use std::cell::Cell;
use std::convert::TryFrom;
use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, Weak};
use std::thread::{self, JoinHandle};

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WindowHandle as RwhWindowHandle, XlibDisplayHandle, XlibWindowHandle,
};

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt as _, CreateWindowAux,
    EventMask, PropMode, Visualid, Window as XWindow, WindowClass,
};
use x11rb::wrapper::ConnectionExt as _;

use super::XcbConnection;
use crate::{
    Event, MouseCursor, Size, WindowEvent, WindowHandler, WindowInfo, WindowOpenOptions,
    WindowScalePolicy,
};

#[cfg(feature = "opengl")]
use crate::gl::{platform, GlContext};
use crate::x11::event_loop::EventLoop;
use crate::x11::visual_info::WindowVisualConfig;

pub struct WindowHandle {
    raw_window_handle: Option<RawWindowHandle>,
    event_loop_handle: Option<JoinHandle<()>>,
    resize_sender: Option<mpsc::Sender<Size>>,
    close_requested: Arc<AtomicBool>,
    is_open: Arc<AtomicBool>,
}

static BLOCKING_EVENT_LOOP_STOP: Mutex<Option<Weak<AtomicBool>>> = Mutex::new(None);

struct BlockingEventLoopRegistration {
    stop_requested: Arc<AtomicBool>,
}

impl BlockingEventLoopRegistration {
    fn new(stop_requested: Arc<AtomicBool>) -> Self {
        if let Ok(mut registered) = BLOCKING_EVENT_LOOP_STOP.lock() {
            *registered = Some(Arc::downgrade(&stop_requested));
        }
        Self { stop_requested }
    }
}

impl Drop for BlockingEventLoopRegistration {
    fn drop(&mut self) {
        if let Ok(mut registered) = BLOCKING_EVENT_LOOP_STOP.lock() {
            let owns_registration = registered
                .as_ref()
                .and_then(Weak::upgrade)
                .is_some_and(|value| Arc::ptr_eq(&value, &self.stop_requested));
            if owns_registration {
                *registered = None;
            }
        }
    }
}

/// Signal only the application-owned blocking X11 loop. Parented editor event
/// loops use separate close flags and cannot observe this request.
pub fn request_event_loop_stop() {
    let stop_requested = BLOCKING_EVENT_LOOP_STOP
        .lock()
        .ok()
        .and_then(|registered| registered.as_ref().and_then(Weak::upgrade));
    if let Some(stop_requested) = stop_requested {
        stop_requested.store(true, Ordering::Release);
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
        self.close_requested.store(true, Ordering::Relaxed);
        if let Some(event_loop) = self.event_loop_handle.take() {
            if event_loop.join().is_err() {
                eprintln!("baseview: X11 window thread panicked while closing");
            }
        }
    }

    pub fn is_open(&self) -> bool {
        self.is_open.load(Ordering::Relaxed)
    }

    pub fn resize(&mut self, size: Size) {
        if let Some(sender) = &self.resize_sender {
            let _ = sender.send(size);
        }
    }
}

impl HasWindowHandle for WindowHandle {
    fn window_handle(&self) -> Result<RwhWindowHandle<'_>, HandleError> {
        if let Some(raw_window_handle) = self.raw_window_handle {
            if self.is_open.load(Ordering::Relaxed) {
                return Ok(unsafe { RwhWindowHandle::borrow_raw(raw_window_handle) });
            }
        }

        Err(HandleError::Unavailable)
    }
}

pub(crate) struct ParentHandle {
    close_requested: Arc<AtomicBool>,
    is_open: Arc<AtomicBool>,
}

impl ParentHandle {
    pub fn new() -> (Self, WindowHandle, mpsc::Receiver<Size>) {
        let close_requested = Arc::new(AtomicBool::new(false));
        let is_open = Arc::new(AtomicBool::new(true));
        let (resize_sender, resize_receiver) = mpsc::channel();
        let handle = WindowHandle {
            raw_window_handle: None,
            event_loop_handle: None,
            resize_sender: Some(resize_sender),
            close_requested: Arc::clone(&close_requested),
            is_open: Arc::clone(&is_open),
        };

        (
            Self {
                close_requested,
                is_open,
            },
            handle,
            resize_receiver,
        )
    }

    pub fn parent_did_drop(&self) -> bool {
        self.close_requested.load(Ordering::Relaxed)
    }
}

impl Drop for ParentHandle {
    fn drop(&mut self) {
        self.is_open.store(false, Ordering::Relaxed);
    }
}

pub(crate) struct WindowInner {
    // GlContext should be dropped **before** XcbConnection is dropped
    #[cfg(feature = "opengl")]
    gl_context: Option<GlContext>,

    pub(crate) xcb_connection: XcbConnection,
    window_id: XWindow,
    pub(crate) window_info: WindowInfo,
    visual_id: Visualid,
    mouse_cursor: Cell<MouseCursor>,

    pub(crate) close_requested: Cell<bool>,
}

pub struct Window<'a> {
    pub(crate) inner: &'a WindowInner,
}

// Hack to allow sending a RawWindowHandle between threads. Do not make public
struct SendableRwh(RawWindowHandle);

unsafe impl Send for SendableRwh {}

type WindowOpenResult = Result<SendableRwh, String>;

impl<'a> Window<'a> {
    pub fn open_parented<P, H, B>(parent: &P, options: WindowOpenOptions, build: B) -> WindowHandle
    where
        P: HasWindowHandle,
        H: WindowHandler + 'static,
        B: FnOnce(&mut crate::Window) -> H,
        B: Send + 'static,
    {
        // Convert parent into something that X understands
        let parent_handle = match parent.window_handle() {
            Ok(handle) => handle,
            Err(error) => {
                eprintln!("baseview: parent X11 window handle is unavailable: {error}");
                return WindowHandle::unavailable();
            }
        };
        let parent_id = match parent_handle.as_raw() {
            RawWindowHandle::Xlib(h) => match u32::try_from(h.window) {
                Ok(window) => window,
                Err(_) => {
                    eprintln!(
                        "baseview: Xlib parent window ID {} does not fit XCB's 32-bit ID",
                        h.window
                    );
                    return WindowHandle::unavailable();
                }
            },
            RawWindowHandle::Xcb(h) => h.window.get(),
            handle => {
                eprintln!("baseview: unsupported X11 parent handle type {handle:?}");
                return WindowHandle::unavailable();
            }
        };

        let (tx, rx) = mpsc::sync_channel::<WindowOpenResult>(1);
        let (parent_handle, mut window_handle, resize_receiver) = ParentHandle::new();
        let initialization_finished = Arc::new(AtomicBool::new(false));
        let worker_initialization_finished = Arc::clone(&initialization_finished);
        let error_tx = tx.clone();
        let join_handle = thread::spawn(move || {
            if let Err(error) = Self::window_thread(
                Some(parent_id),
                options,
                build,
                tx,
                Some(parent_handle),
                Some(resize_receiver),
                None,
                worker_initialization_finished,
            ) {
                let message = error.to_string();
                if initialization_finished.load(Ordering::Acquire)
                    || error_tx.try_send(Err(message.clone())).is_err()
                {
                    eprintln!("baseview: X11 window thread failed: {message}");
                }
            }
        });

        match rx.recv() {
            Ok(Ok(raw_window_handle)) => {
                window_handle.raw_window_handle = Some(raw_window_handle.0);
                window_handle.event_loop_handle = Some(join_handle);
            }
            Ok(Err(error)) => {
                eprintln!("baseview: could not open parented X11 window: {error}");
                if join_handle.join().is_err() {
                    eprintln!("baseview: X11 window thread panicked during initialization");
                }
            }
            Err(error) => {
                eprintln!("baseview: X11 window thread exited before initialization: {error}");
                if join_handle.join().is_err() {
                    eprintln!("baseview: X11 window thread panicked during initialization");
                }
            }
        }
        window_handle
    }

    /// Open a top-level window and return immediately.
    ///
    /// This is what a plugin needs for a floating editor. X11 makes it the
    /// easiest of the three platforms: the window already runs on its own
    /// thread even in the parented case, so a floating window is
    /// [`Self::open_parented`]'s structure with no parent id — the difference
    /// from [`Self::open_blocking`] is only that we do not join the thread.
    ///
    /// No `stop_requested` flag is passed: that exists so a standalone host can
    /// ask the loop to exit, whereas here the returned [`WindowHandle`] owns
    /// the window's lifetime and closing it tears the loop down.
    pub fn open_floating<H, B>(options: WindowOpenOptions, build: B) -> WindowHandle
    where
        H: WindowHandler + 'static,
        B: FnOnce(&mut crate::Window) -> H,
        B: Send + 'static,
    {
        let (tx, rx) = mpsc::sync_channel::<WindowOpenResult>(1);
        let (parent_handle, mut window_handle, resize_receiver) = ParentHandle::new();
        let initialization_finished = Arc::new(AtomicBool::new(false));
        let worker_initialization_finished = Arc::clone(&initialization_finished);
        let error_tx = tx.clone();
        let join_handle = thread::spawn(move || {
            if let Err(error) = Self::window_thread(
                // `None` is what makes it top-level rather than embedded.
                None,
                options,
                build,
                tx,
                Some(parent_handle),
                Some(resize_receiver),
                None,
                worker_initialization_finished,
            ) {
                let message = error.to_string();
                if initialization_finished.load(Ordering::Acquire)
                    || error_tx.try_send(Err(message.clone())).is_err()
                {
                    eprintln!("baseview: floating X11 window thread failed: {message}");
                }
            }
        });

        match rx.recv() {
            Ok(Ok(raw_window_handle)) => {
                window_handle.raw_window_handle = Some(raw_window_handle.0);
                window_handle.event_loop_handle = Some(join_handle);
            }
            Ok(Err(error)) => {
                eprintln!("baseview: could not open floating X11 window: {error}");
                if join_handle.join().is_err() {
                    eprintln!("baseview: X11 window thread panicked during initialization");
                }
            }
            Err(error) => {
                eprintln!("baseview: X11 window thread exited before initialization: {error}");
                if join_handle.join().is_err() {
                    eprintln!("baseview: X11 window thread panicked during initialization");
                }
            }
        }
        window_handle
    }

    pub fn open_blocking<H, B>(options: WindowOpenOptions, build: B)
    where
        H: WindowHandler + 'static,
        B: FnOnce(&mut crate::Window) -> H,
        B: Send + 'static,
    {
        let (tx, rx) = mpsc::sync_channel::<WindowOpenResult>(1);
        let stop_requested = Arc::new(AtomicBool::new(false));
        let _blocking_event_loop = BlockingEventLoopRegistration::new(Arc::clone(&stop_requested));
        let initialization_finished = Arc::new(AtomicBool::new(false));
        let worker_initialization_finished = Arc::clone(&initialization_finished);

        let error_tx = tx.clone();
        let thread = thread::spawn(move || {
            if let Err(error) = Self::window_thread(
                None,
                options,
                build,
                tx,
                None,
                None,
                Some(stop_requested),
                worker_initialization_finished,
            ) {
                let message = error.to_string();
                if initialization_finished.load(Ordering::Acquire)
                    || error_tx.try_send(Err(message.clone())).is_err()
                {
                    eprintln!("baseview: blocking X11 window thread failed: {message}");
                }
            }
        });

        match rx.recv() {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                eprintln!("baseview: could not open blocking X11 window: {error}");
                if thread.join().is_err() {
                    eprintln!(
                        "baseview: blocking X11 window thread panicked during initialization"
                    );
                }
                return;
            }
            Err(error) => {
                eprintln!(
                    "baseview: blocking X11 window thread exited before initialization: {error}"
                );
                if thread.join().is_err() {
                    eprintln!(
                        "baseview: blocking X11 window thread panicked during initialization"
                    );
                }
                return;
            }
        }

        thread.join().unwrap_or_else(|err| {
            eprintln!("Window thread panicked: {:#?}", err);
        });
    }

    fn window_thread<H, B>(
        parent: Option<u32>,
        options: WindowOpenOptions,
        build: B,
        tx: mpsc::SyncSender<WindowOpenResult>,
        parent_handle: Option<ParentHandle>,
        resize_receiver: Option<mpsc::Receiver<Size>>,
        blocking_stop_requested: Option<Arc<AtomicBool>>,
        initialization_finished: Arc<AtomicBool>,
    ) -> Result<(), Box<dyn Error>>
    where
        H: WindowHandler + 'static,
        B: FnOnce(&mut crate::Window) -> H,
        B: Send + 'static,
    {
        // Connect to the X server.
        let xcb_connection = XcbConnection::new()?;

        // Get screen information
        let screen = xcb_connection.screen();
        let parent_id = parent.unwrap_or(screen.root);

        let scaling = match options.scale {
            WindowScalePolicy::SystemScaleFactor => xcb_connection.get_scaling().unwrap_or(1.0),
            WindowScalePolicy::ScaleFactor(scale) => scale,
        };

        let window_info = WindowInfo::from_logical_size(options.size, scaling);

        #[cfg(feature = "opengl")]
        let visual_info =
            WindowVisualConfig::find_best_visual_config_for_gl(&xcb_connection, options.gl_config)?;

        #[cfg(not(feature = "opengl"))]
        let visual_info = WindowVisualConfig::find_best_visual_config(&xcb_connection)?;

        let window_id = xcb_connection.conn.generate_id()?;
        xcb_connection
            .conn
            .create_window(
                visual_info.visual_depth,
                window_id,
                parent_id,
                0,                                         // x coordinate of the new window
                0,                                         // y coordinate of the new window
                window_info.physical_size().width as u16,  // window width
                window_info.physical_size().height as u16, // window height
                0,                                         // window border
                WindowClass::INPUT_OUTPUT,
                visual_info.visual_id,
                &CreateWindowAux::new()
                    .event_mask(
                        EventMask::EXPOSURE
                            | EventMask::POINTER_MOTION
                            | EventMask::BUTTON_PRESS
                            | EventMask::BUTTON_RELEASE
                            | EventMask::KEY_PRESS
                            | EventMask::KEY_RELEASE
                            | EventMask::STRUCTURE_NOTIFY
                            | EventMask::ENTER_WINDOW
                            | EventMask::LEAVE_WINDOW,
                    )
                    // As mentioned above, these two values are needed to be able to create a window
                    // with a depth of 32-bits when the parent window has a different depth
                    .colormap(visual_info.color_map)
                    .border_pixel(0),
            )?
            .check()?;
        xcb_connection.conn.map_window(window_id)?.check()?;

        // Change window title
        let title = options.title;
        xcb_connection
            .conn
            .change_property8(
                PropMode::REPLACE,
                window_id,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                title.as_bytes(),
            )?
            .check()?;

        xcb_connection
            .conn
            .change_property32(
                PropMode::REPLACE,
                window_id,
                xcb_connection.atoms.WM_PROTOCOLS,
                AtomEnum::ATOM,
                &[xcb_connection.atoms.WM_DELETE_WINDOW],
            )?
            .check()?;

        xcb_connection.conn.flush()?;

        #[cfg(feature = "opengl")]
        let gl_context = visual_info
            .fb_config
            .map(|fb_config| -> Result<GlContext, crate::gl::GlError> {
                use std::ffi::c_ulong;

                let window = window_id as c_ulong;
                let display = xcb_connection.dpy;

                // Because of the visual negotation we had to take some extra steps to create this context
                let context = unsafe { platform::GlContext::create(window, display, fb_config) }?;
                Ok(GlContext::new(context))
            })
            .transpose()?;

        let mut inner = WindowInner {
            xcb_connection,
            window_id,
            window_info,
            visual_id: visual_info.visual_id,
            mouse_cursor: Cell::new(MouseCursor::default()),

            close_requested: Cell::new(false),

            #[cfg(feature = "opengl")]
            gl_context,
        };

        let mut window = crate::Window::new(Window { inner: &mut inner });

        let mut handler = build(&mut window);

        // Send an initial window resized event so the user is alerted of
        // the correct dpi scaling.
        handler.on_event(
            &mut window,
            Event::Window(WindowEvent::Resized(window_info)),
        );

        let raw_window_handle = window
            .window_handle()
            .map_err(|error| {
                IoError::new(
                    ErrorKind::Other,
                    format!("new X11 window handle is unavailable: {error}"),
                )
            })?
            .as_raw();
        tx.send(Ok(SendableRwh(raw_window_handle))).map_err(|_| {
            IoError::new(
                ErrorKind::BrokenPipe,
                "X11 window caller stopped waiting during initialization",
            )
        })?;
        initialization_finished.store(true, Ordering::Release);

        EventLoop::new(
            inner,
            handler,
            parent_handle,
            resize_receiver,
            blocking_stop_requested,
        )
        .run()?;

        Ok(())
    }

    pub fn set_mouse_cursor(&self, mouse_cursor: MouseCursor) {
        if self.inner.mouse_cursor.get() == mouse_cursor {
            return;
        }

        let xid = match self.inner.xcb_connection.get_cursor(mouse_cursor) {
            Ok(xid) => xid,
            Err(error) => {
                eprintln!("baseview: could not load X11 cursor {mouse_cursor:?}: {error}");
                return;
            }
        };

        if xid != 0 {
            let _ = self.inner.xcb_connection.conn.change_window_attributes(
                self.inner.window_id,
                &ChangeWindowAttributesAux::new().cursor(xid),
            );
            let _ = self.inner.xcb_connection.conn.flush();
        }

        self.inner.mouse_cursor.set(mouse_cursor);
    }

    pub fn close(&mut self) {
        self.inner.close_requested.set(true);
    }

    pub fn has_focus(&mut self) -> bool {
        self.inner
            .xcb_connection
            .conn
            .get_input_focus()
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| reply.focus == self.inner.window_id)
            .unwrap_or(false)
    }

    pub fn focus(&mut self) {
        let _ = self.inner.xcb_connection.conn.set_input_focus(
            x11rb::protocol::xproto::InputFocus::PARENT,
            self.inner.window_id,
            x11rb::CURRENT_TIME,
        );
        let _ = self.inner.xcb_connection.conn.flush();
    }

    pub fn resize(&mut self, size: Size) {
        let scaling = self.inner.window_info.scale();
        let new_window_info = WindowInfo::from_logical_size(size, scaling);

        let _ = self.inner.xcb_connection.conn.configure_window(
            self.inner.window_id,
            &ConfigureWindowAux::new()
                .width(new_window_info.physical_size().width)
                .height(new_window_info.physical_size().height),
        );
        let _ = self.inner.xcb_connection.conn.flush();

        // This will trigger a `ConfigureNotify` event which will in turn change `self.window_info`
        // and notify the window handler about it
    }

    #[cfg(feature = "opengl")]
    pub fn gl_context(&self) -> Option<&crate::gl::GlContext> {
        self.inner.gl_context.as_ref()
    }
}

impl HasWindowHandle for Window<'_> {
    fn window_handle(&self) -> Result<RwhWindowHandle<'_>, HandleError> {
        let mut handle = XlibWindowHandle::new(self.inner.window_id.into());
        handle.visual_id = self.inner.visual_id.into();
        let handle = RawWindowHandle::Xlib(handle);
        Ok(unsafe { RwhWindowHandle::borrow_raw(handle) })
    }
}

impl HasDisplayHandle for Window<'_> {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let display = self.inner.xcb_connection.dpy;
        let display = NonNull::new(display.cast());
        let screen = unsafe { x11::xlib::XDefaultScreen(self.inner.xcb_connection.dpy) };
        let handle = RawDisplayHandle::Xlib(XlibDisplayHandle::new(display, screen));
        Ok(unsafe { DisplayHandle::borrow_raw(handle) })
    }
}

/// Clipboard support is not part of the Phase-1 native editor contract.
/// Keep this entry point non-panicking until the platform clipboard service is implemented.
pub fn copy_to_clipboard(_data: &str) {}
