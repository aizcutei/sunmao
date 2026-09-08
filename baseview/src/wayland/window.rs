use std::cell::Cell;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle, WindowHandle as RwhWindowHandle,
};
use wayland_client::protocol::{wl_callback, wl_compositor, wl_registry, wl_shm, wl_surface};
use wayland_client::{delegate_noop, Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

use crate::{
    Event, MouseCursor, Size, WindowEvent, WindowHandler, WindowInfo, WindowOpenOptions,
    WindowScalePolicy,
};

pub struct WindowHandle {
    raw_window_handle: Option<RawWindowHandle>,
    event_loop_handle: Option<JoinHandle<()>>,
    resize_sender: Option<mpsc::Sender<Size>>,
    close_requested: Arc<AtomicBool>,
    is_open: Arc<AtomicBool>,
}

struct OpenFlag(Arc<AtomicBool>);

impl Drop for OpenFlag {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl WindowHandle {
    fn unavailable() -> Self {
        Self {
            raw_window_handle: None,
            event_loop_handle: None,
            resize_sender: None,
            close_requested: Arc::new(AtomicBool::new(false)),
            is_open: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn close(&mut self) {
        self.close_requested.store(true, Ordering::Release);
        if let Some(thread) = self.event_loop_handle.take() {
            if thread.join().is_err() {
                eprintln!("baseview: Wayland window thread panicked while closing");
            }
        }
    }

    pub fn is_open(&self) -> bool {
        self.is_open.load(Ordering::Acquire)
    }

    pub fn resize(&mut self, size: Size) {
        if let Some(sender) = &self.resize_sender {
            let _ = sender.send(size);
        }
    }
}

impl HasWindowHandle for WindowHandle {
    fn window_handle(&self) -> Result<RwhWindowHandle<'_>, HandleError> {
        match self.raw_window_handle {
            Some(raw) if self.is_open() => Ok(unsafe { RwhWindowHandle::borrow_raw(raw) }),
            _ => Err(HandleError::Unavailable),
        }
    }
}

pub(super) struct OpenState {
    pub(super) activation: super::activation::Activation,
    pub(super) seats: std::collections::HashMap<u32, super::pointer::Seat>,
    pub(super) events: Vec<Event>,
    compositor: Option<wl_compositor::WlCompositor>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    shm: Option<wl_shm::WlShm>,
    cursor_theme: Option<wayland_cursor::CursorTheme>,
    configured: bool,
    pub(super) close_requested: bool,
    configured_size: Option<(u32, u32)>,
}

impl Default for OpenState {
    fn default() -> Self {
        Self {
            activation: Default::default(),
            seats: Default::default(),
            events: Vec::new(),
            compositor: None,
            wm_base: None,
            shm: None,
            cursor_theme: None,
            configured: false,
            close_requested: false,
            configured_size: None,
        }
    }
}

impl OpenState {
    fn update_cursors(
        &mut self,
        requested: MouseCursor,
        connection: &Connection,
        handle: &QueueHandle<Self>,
    ) -> Result<(), String> {
        if !self.seats.values().any(|seat| seat.cursor.entered()) {
            return Ok(());
        }
        let compositor = self
            .compositor
            .as_ref()
            .ok_or("missing cursor compositor")?;
        if requested != MouseCursor::Hidden && self.cursor_theme.is_none() {
            let shm = self
                .shm
                .clone()
                .ok_or("wl_shm is unavailable for cursor images")?;
            self.cursor_theme = Some(
                wayland_cursor::CursorTheme::load(connection, shm, 24)
                    .map_err(|error| error.to_string())?,
            );
        }
        for seat in self.seats.values_mut() {
            if let Some(pointer) = &seat.pointer {
                seat.cursor.update(
                    pointer,
                    requested,
                    self.cursor_theme.as_mut(),
                    compositor,
                    handle,
                    Instant::now(),
                )?;
            }
        }
        Ok(())
    }
}

impl Dispatch<wl_callback::WlCallback, Arc<AtomicBool>> for OpenState {
    fn event(
        _: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        done: &Arc<AtomicBool>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        done.store(true, Ordering::Release);
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for OpenState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::GlobalRemove { name } = event {
            state.activation.remove_global(name);
            if let Some(mut seat) = state.seats.remove(&name) {
                seat.remove_pointer(&mut state.events);
                let focused = seat.keyboard.as_ref().is_some_and(|k| k.focused);
                seat.remove_keyboard(&mut state.events);
                if focused
                    && !state
                        .seats
                        .values()
                        .any(|seat| seat.keyboard.as_ref().is_some_and(|k| k.focused))
                {
                    state.events.push(Event::Window(WindowEvent::Unfocused));
                }
            }
            return;
        }
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "xdg_activation_v1" => {
                    state.activation.manager = Some((name, registry.bind(name, 1, handle, ())));
                }
                "wl_seat" => {
                    state.seats.insert(
                        name,
                        super::pointer::Seat::new(registry.bind(
                            name,
                            version.min(5),
                            handle,
                            name,
                        )),
                    );
                }
                "wl_shm" => {
                    state.shm = Some(registry.bind(name, 1, handle, ()));
                }
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(4), handle, ()))
                }
                "xdg_wm_base" => {
                    state.wm_base = Some(registry.bind(name, version.min(3), handle, ()))
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for OpenState {
    fn event(
        _: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for OpenState {
    fn event(
        state: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
            state.configured = true;
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for OpenState {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            xdg_toplevel::Event::Configure { width, height, .. } if width > 0 && height > 0 => {
                state.configured_size = Some((width as u32, height as u32));
            }
            xdg_toplevel::Event::Close => state.close_requested = true,
            _ => {}
        }
    }
}

delegate_noop!(OpenState: ignore wl_compositor::WlCompositor);
delegate_noop!(OpenState: ignore wl_shm::WlShm);
delegate_noop!(OpenState: ignore wl_surface::WlSurface);

pub(crate) struct WindowInner {
    // EGL must be destroyed before the protocol objects and connection.
    #[cfg(feature = "opengl")]
    gl_context: Option<crate::gl::GlContext>,
    connection: Connection,
    surface: wl_surface::WlSurface,
    shell_surface: xdg_surface::XdgSurface,
    toplevel: xdg_toplevel::XdgToplevel,
    window_info: Cell<WindowInfo>,
    close_requested: Cell<bool>,
    focused: Cell<bool>,
    focus_requested: Cell<bool>,
    cursor: Cell<MouseCursor>,
}

impl Drop for WindowInner {
    fn drop(&mut self) {
        #[cfg(feature = "opengl")]
        drop(self.gl_context.take());
        self.toplevel.destroy();
        self.shell_surface.destroy();
        self.surface.destroy();
        let _ = self.connection.flush();
    }
}

pub struct Window<'a> {
    inner: &'a WindowInner,
}

struct SendableRawWindowHandle(RawWindowHandle);
unsafe impl Send for SendableRawWindowHandle {}

type OpenResult = Result<SendableRawWindowHandle, String>;

impl<'a> Window<'a> {
    pub fn open_floating<H, B>(options: WindowOpenOptions, build: B) -> WindowHandle
    where
        H: WindowHandler + 'static,
        B: FnOnce(&mut crate::Window) -> H + Send + 'static,
    {
        let (result_sender, result_receiver) = mpsc::sync_channel::<OpenResult>(1);
        let (resize_sender, resize_receiver) = mpsc::channel();
        let close_requested = Arc::new(AtomicBool::new(false));
        let worker_close_requested = Arc::clone(&close_requested);
        let is_open = Arc::new(AtomicBool::new(false));
        let worker_is_open = Arc::clone(&is_open);
        let error_sender = result_sender.clone();

        let thread = thread::spawn(move || {
            let _open_flag = OpenFlag(Arc::clone(&worker_is_open));
            let result = Self::window_thread(
                options,
                build,
                result_sender,
                resize_receiver,
                worker_close_requested,
                worker_is_open,
            );
            if let Err(error) = result {
                let _ = error_sender.try_send(Err(error.clone()));
                eprintln!("baseview: Wayland window thread failed: {error}");
            }
        });

        match result_receiver.recv() {
            Ok(Ok(raw)) => WindowHandle {
                raw_window_handle: Some(raw.0),
                event_loop_handle: Some(thread),
                resize_sender: Some(resize_sender),
                close_requested,
                is_open,
            },
            Ok(Err(error)) => {
                eprintln!("baseview: could not open floating Wayland window: {error}");
                let _ = thread.join();
                WindowHandle::unavailable()
            }
            Err(error) => {
                eprintln!("baseview: Wayland window exited during initialization: {error}");
                let _ = thread.join();
                WindowHandle::unavailable()
            }
        }
    }

    fn window_thread<H, B>(
        options: WindowOpenOptions,
        build: B,
        result_sender: mpsc::SyncSender<OpenResult>,
        resize_receiver: mpsc::Receiver<Size>,
        close_requested: Arc<AtomicBool>,
        is_open: Arc<AtomicBool>,
    ) -> Result<(), String>
    where
        H: WindowHandler + 'static,
        B: FnOnce(&mut crate::Window) -> H,
    {
        let connection = Connection::connect_to_env().map_err(|e| e.to_string())?;
        let mut queue = connection.new_event_queue();
        let handle = queue.handle();
        connection.display().get_registry(&handle, ());
        let mut state = OpenState::default();
        super::dispatch::roundtrip(&connection, &mut queue, &mut state, Duration::from_secs(5))?;
        let compositor = state
            .compositor
            .clone()
            .ok_or("wl_compositor is unavailable")?;
        let wm_base = state.wm_base.clone().ok_or("xdg_wm_base is unavailable")?;

        let scale = match options.scale {
            WindowScalePolicy::SystemScaleFactor => 1.0,
            WindowScalePolicy::ScaleFactor(scale) if scale.is_finite() && scale > 0.0 => scale,
            WindowScalePolicy::ScaleFactor(_) => return Err("invalid window scale".into()),
        };
        let mut info = WindowInfo::from_logical_size(options.size, scale);
        let surface = compositor.create_surface(&handle, ());
        let shell_surface = wm_base.get_xdg_surface(&surface, &handle, ());
        let toplevel = shell_surface.get_toplevel(&handle, ());
        toplevel.set_title(options.title);
        toplevel.set_app_id("sunmao".into());
        surface.commit();
        super::dispatch::roundtrip(&connection, &mut queue, &mut state, Duration::from_secs(5))?;
        if state.close_requested {
            return Err(
                "Wayland initialization was rejected; see protocol/input diagnostics".into(),
            );
        }
        if !state.configured {
            return Err("compositor did not configure the Wayland surface".into());
        }

        #[cfg(feature = "opengl")]
        let gl_context = options
            .gl_config
            .as_ref()
            .map(|config| {
                let size = info.physical_size();
                super::egl::Context::new(
                    &connection,
                    &surface,
                    size.width as i32,
                    size.height as i32,
                    config,
                )
                .map(crate::gl::GlContext::from_wayland)
            })
            .transpose()?;

        let inner = WindowInner {
            #[cfg(feature = "opengl")]
            gl_context,
            connection,
            surface,
            shell_surface,
            toplevel,
            window_info: Cell::new(info),
            close_requested: Cell::new(false),
            focused: Cell::new(false),
            focus_requested: Cell::new(false),
            cursor: Cell::new(MouseCursor::Default),
        };
        let mut window = crate::Window::new(Window { inner: &inner });
        let mut handler = build(&mut window);
        handler.on_event(&mut window, Event::Window(WindowEvent::Resized(info)));

        let raw = window.window_handle().map_err(|e| e.to_string())?.as_raw();
        is_open.store(true, Ordering::Release);
        result_sender
            .send(Ok(SendableRawWindowHandle(raw)))
            .map_err(|_| "window caller stopped waiting during initialization".to_string())?;

        let frame_interval = Duration::from_millis(15);
        let mut last_frame = Instant::now() - frame_interval;
        while !close_requested.load(Ordering::Acquire)
            && !inner.close_requested.get()
            && !state.close_requested
        {
            for seat in state.seats.values_mut() {
                if let Some(keyboard) = seat.keyboard.as_mut() {
                    keyboard.tick(Instant::now(), &mut state.events);
                }
            }
            for event in state.events.drain(..) {
                match &event {
                    Event::Window(WindowEvent::Focused) => inner.focused.set(true),
                    Event::Window(WindowEvent::Unfocused) => inner.focused.set(false),
                    _ => {}
                }
                handler.on_event(&mut window, event);
            }
            if let Some(size) = resize_receiver.try_iter().last() {
                window.resize(size);
                info = inner.window_info.get();
                handler.on_event(&mut window, Event::Window(WindowEvent::Resized(info)));
            }
            if let Some((width, height)) = state.configured_size.take() {
                let configured =
                    WindowInfo::from_physical_size(crate::PhySize::new(width, height), scale);
                if configured.physical_size() != inner.window_info.get().physical_size() {
                    inner.window_info.set(configured);
                    #[cfg(feature = "opengl")]
                    if let Some(context) = inner.gl_context.as_ref() {
                        context.resize_wayland(width as i32, height as i32);
                    }
                    handler.on_event(&mut window, Event::Window(WindowEvent::Resized(configured)));
                }
            }
            if last_frame.elapsed() >= frame_interval {
                handler.on_frame(&mut window);
                last_frame = Instant::now();
            }
            state.update_activation(
                inner.focus_requested.replace(false),
                &inner.surface,
                &handle,
            );
            state.update_cursors(inner.cursor.get(), &inner.connection, &handle)?;
            super::dispatch::dispatch_for(
                &mut queue,
                &mut state,
                frame_interval.saturating_sub(last_frame.elapsed()),
            )?;
        }

        handler.on_event(&mut window, Event::Window(WindowEvent::WillClose));
        drop(handler);
        is_open.store(false, Ordering::Release);
        Ok(())
    }

    pub fn close(&mut self) {
        self.inner.close_requested.set(true);
    }

    pub fn resize(&mut self, size: Size) {
        let scale = self.inner.window_info.get().scale();
        let info = WindowInfo::from_logical_size(size, scale);
        let physical = info.physical_size();
        self.inner.window_info.set(info);
        self.inner
            .toplevel
            .set_min_size(physical.width as i32, physical.height as i32);
        self.inner
            .toplevel
            .set_max_size(physical.width as i32, physical.height as i32);
        #[cfg(feature = "opengl")]
        if let Some(context) = self.inner.gl_context.as_ref() {
            context.resize_wayland(physical.width as i32, physical.height as i32);
        }
        self.inner.surface.commit();
    }

    pub fn set_mouse_cursor(&mut self, cursor: MouseCursor) {
        self.inner.cursor.set(cursor);
    }

    pub fn has_focus(&mut self) -> bool {
        self.inner.focused.get()
    }

    pub fn focus(&mut self) {
        self.inner.focus_requested.set(true);
    }

    #[cfg(feature = "opengl")]
    pub fn gl_context(&self) -> Option<&crate::gl::GlContext> {
        self.inner.gl_context.as_ref()
    }
}

impl HasWindowHandle for Window<'_> {
    fn window_handle(&self) -> Result<RwhWindowHandle<'_>, HandleError> {
        let surface = NonNull::new(self.inner.surface.id().as_ptr().cast())
            .ok_or(HandleError::Unavailable)?;
        let raw = RawWindowHandle::Wayland(WaylandWindowHandle::new(surface));
        Ok(unsafe { RwhWindowHandle::borrow_raw(raw) })
    }
}

impl HasDisplayHandle for Window<'_> {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let display = NonNull::new(self.inner.connection.backend().display_ptr().cast())
            .ok_or(HandleError::Unavailable)?;
        let raw = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display));
        Ok(unsafe { DisplayHandle::borrow_raw(raw) })
    }
}
