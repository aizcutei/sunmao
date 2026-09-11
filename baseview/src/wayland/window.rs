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

use crate::{Event, MouseCursor, Size, WindowEvent, WindowHandler, WindowInfo, WindowOpenOptions};

enum WindowCommand {
    Resize(Size),
    Title(String, mpsc::SyncSender<bool>),
}

pub struct WindowHandle {
    raw_window_handle: Option<RawWindowHandle>,
    event_loop_handle: Option<JoinHandle<()>>,
    resize_sender: Option<mpsc::Sender<WindowCommand>>,
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
    pub fn set_title(&mut self, title: &str) -> bool {
        if !self.is_open() || title.contains('\0') {
            return false;
        }
        let Some(sender) = &self.resize_sender else {
            return false;
        };
        let (reply, result) = mpsc::sync_channel(1);
        sender
            .send(WindowCommand::Title(title.into(), reply))
            .is_ok()
            && result.recv().unwrap_or(false)
    }

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
            let _ = sender.send(WindowCommand::Resize(size));
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
    pub(super) scaling: super::scaling::Scaling,
    pub(super) activation: super::activation::Activation,
    pub(super) seats: std::collections::HashMap<u32, super::pointer::Seat>,
    pub(super) events: Vec<Event>,
    compositor: Option<wl_compositor::WlCompositor>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    shm: Option<wl_shm::WlShm>,
    cursor_theme: Option<wayland_cursor::CursorTheme>,
    cursor_theme_scale: i32,
    configured: bool,
    pub(super) close_requested: bool,
    pending_size: Option<(i32, i32)>,
    configured_size: Option<(i32, i32)>,
}

impl Default for OpenState {
    fn default() -> Self {
        Self {
            scaling: Default::default(),
            activation: Default::default(),
            seats: Default::default(),
            events: Vec::new(),
            compositor: None,
            wm_base: None,
            shm: None,
            cursor_theme: None,
            cursor_theme_scale: 1,
            configured: false,
            close_requested: false,
            pending_size: None,
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
        let scale = if compositor.version() >= 3 {
            self.scaling
                .model
                .system_scale(
                    self.scaling.viewporter.is_some() && self.scaling.fractional.is_some(),
                )
                .ceil() as i32
        } else {
            1
        };
        if self.cursor_theme_scale != scale {
            self.cursor_theme = None;
            self.cursor_theme_scale = scale;
        }
        if requested != MouseCursor::Hidden && self.cursor_theme.is_none() {
            let shm = self
                .shm
                .clone()
                .ok_or("wl_shm is unavailable for cursor images")?;
            self.cursor_theme = Some(
                // `load` lets XCURSOR_SIZE override the physical size, losing
                // output scaling. Interpret that setting in logical units once.
                wayland_cursor::CursorTheme::load_from_name(
                    connection,
                    shm,
                    &std::env::var("XCURSOR_THEME").unwrap_or_else(|_| "default".into()),
                    super::cursor::theme_size(
                        std::env::var("XCURSOR_SIZE").ok().as_deref(),
                        scale,
                    )?,
                )
                .map_err(|error| error.to_string())?,
            );
        }
        for seat in self.seats.values_mut() {
            if let Some(pointer) = &seat.pointer {
                seat.cursor.update(
                    pointer,
                    requested,
                    scale,
                    self.scaling.viewporter.as_ref().map(|(_, p)| p),
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
            state.scaling.remove(name);
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
                "wl_output" => {
                    state.scaling.outputs.insert(
                        name,
                        super::scaling::Output {
                            proxy: registry.bind(name, version.min(4), handle, name),
                            pending: 1,
                        },
                    );
                }
                "wp_viewporter" => {
                    state.scaling.viewporter = Some((name, registry.bind(name, 1, handle, ())));
                }
                "wp_fractional_scale_manager_v1" => {
                    state.scaling.fractional = Some((name, registry.bind(name, 1, handle, ())));
                }
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
                    state.compositor = Some(registry.bind(name, version.min(6), handle, ()))
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
            if let Some(size) = state.pending_size.take() {
                state.configured_size = Some(size);
            }
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
            xdg_toplevel::Event::Configure { width, height, .. } => {
                state.pending_size = Some((width, height));
            }
            xdg_toplevel::Event::Close => state.close_requested = true,
            _ => {}
        }
    }
}

delegate_noop!(OpenState: ignore wl_compositor::WlCompositor);
delegate_noop!(OpenState: ignore wl_shm::WlShm);

pub(crate) struct WindowInner {
    // EGL must be destroyed before the protocol objects and connection.
    #[cfg(feature = "opengl")]
    gl_context: Option<crate::gl::GlContext>,
    connection: Connection,
    surface_scaling: super::scaling::SurfaceScaling,
    surface: wl_surface::WlSurface,
    shell_surface: xdg_surface::XdgSurface,
    toplevel: xdg_toplevel::XdgToplevel,
    window_info: Cell<WindowInfo>,
    close_requested: Cell<bool>,
    focused: Cell<bool>,
    focus_requested: Cell<bool>,
    cursor: Cell<MouseCursor>,
}

impl WindowInner {
    fn apply_geometry(&self, info: WindowInfo) -> Result<(), String> {
        self.surface_scaling.apply(&self.surface, info)?;
        self.window_info.set(info);
        #[cfg(feature = "opengl")]
        if let Some(context) = self.gl_context.as_ref() {
            let size = info.physical_size();
            context.resize_wayland(size.width as i32, size.height as i32);
        }
        Ok(())
    }
}

impl Drop for WindowInner {
    fn drop(&mut self) {
        #[cfg(feature = "opengl")]
        drop(self.gl_context.take());
        drop(std::mem::take(&mut self.surface_scaling));
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
        if options.transient_parent.is_some() {
            return WindowHandle::unavailable();
        }
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
        resize_receiver: mpsc::Receiver<WindowCommand>,
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

        let surface = compositor.create_surface(&handle, ());
        state.scaling.surface = Some(surface.clone());
        let surface_scaling =
            super::scaling::SurfaceScaling::new(&state.scaling, &surface, &handle);
        let shell_surface = wm_base.get_xdg_surface(&surface, &handle, ());
        let toplevel = shell_surface.get_toplevel(&handle, ());
        toplevel.set_title(options.title.clone());
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

        let scale = surface_scaling.scale(&state.scaling.model, options.scale, &surface);
        let logical = state.configured_size.take().map_or(options.size, |(w, h)| {
            super::scaling::configured_size(options.size, w, h)
        });
        let info = super::scaling::geometry(logical, scale)?;
        surface_scaling.apply(&surface, info)?;

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
            surface_scaling,
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
            // Apply compositor geometry/scale before callbacks can draw a new
            // buffer. Pointer coordinates already use these surface units.
            let old = inner.window_info.get();
            let logical = state
                .configured_size
                .take()
                .map_or(old.logical_size(), |(w, h)| {
                    super::scaling::configured_size(old.logical_size(), w, h)
                });
            let scale =
                inner
                    .surface_scaling
                    .scale(&state.scaling.model, options.scale, &inner.surface);
            let configured = super::scaling::geometry(logical, scale)?;
            if configured.logical_size() != old.logical_size() || configured.scale() != old.scale()
            {
                inner.apply_geometry(configured)?;
                handler.on_event(&mut window, Event::Window(WindowEvent::Resized(configured)));
            }
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
            for command in resize_receiver.try_iter() {
                match command {
                    WindowCommand::Resize(size) => {
                        window.resize(size);
                        handler.on_event(
                            &mut window,
                            Event::Window(WindowEvent::Resized(inner.window_info.get())),
                        );
                    }
                    WindowCommand::Title(title, reply) => {
                        toplevel.set_title(title);
                        let _ = reply.send(inner.connection.flush().is_ok());
                    }
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
        let result = super::scaling::geometry(size, self.inner.window_info.get().scale())
            .and_then(|info| self.inner.apply_geometry(info));
        if let Err(error) = result {
            eprintln!("baseview: Wayland resize rejected: {error}");
            return;
        }
        let logical = self.inner.window_info.get().logical_size();
        self.inner
            .toplevel
            .set_min_size(logical.width as i32, logical.height as i32);
        self.inner
            .toplevel
            .set_max_size(logical.width as i32, logical.height as i32);
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
