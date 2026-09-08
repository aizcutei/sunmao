use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle as RwhWindowHandle,
};

use crate::{MouseCursor, Size, WindowHandler, WindowOpenOptions};

pub enum WindowHandle {
    X11(crate::x11::WindowHandle),
    #[cfg(feature = "wayland")]
    Wayland(crate::wayland::window::WindowHandle),
}

impl WindowHandle {
    pub fn close(&mut self) {
        match self {
            Self::X11(handle) => handle.close(),
            #[cfg(feature = "wayland")]
            Self::Wayland(handle) => handle.close(),
        }
    }

    pub fn is_open(&self) -> bool {
        match self {
            Self::X11(handle) => handle.is_open(),
            #[cfg(feature = "wayland")]
            Self::Wayland(handle) => handle.is_open(),
        }
    }

    pub fn resize(&mut self, size: Size) {
        match self {
            Self::X11(handle) => handle.resize(size),
            #[cfg(feature = "wayland")]
            Self::Wayland(handle) => handle.resize(size),
        }
    }
}

impl HasWindowHandle for WindowHandle {
    fn window_handle(&self) -> Result<RwhWindowHandle<'_>, HandleError> {
        match self {
            Self::X11(handle) => handle.window_handle(),
            #[cfg(feature = "wayland")]
            Self::Wayland(handle) => handle.window_handle(),
        }
    }
}

pub enum Window<'a> {
    X11(crate::x11::Window<'a>),
    #[cfg(feature = "wayland")]
    Wayland(crate::wayland::window::Window<'a>),
}

impl<'a> From<crate::x11::Window<'a>> for Window<'a> {
    fn from(window: crate::x11::Window<'a>) -> Self {
        Self::X11(window)
    }
}

#[cfg(feature = "wayland")]
impl<'a> From<crate::wayland::window::Window<'a>> for Window<'a> {
    fn from(window: crate::wayland::window::Window<'a>) -> Self {
        Self::Wayland(window)
    }
}

impl<'a> Window<'a> {
    pub fn open_parented<P, H, B>(parent: &P, options: WindowOpenOptions, build: B) -> WindowHandle
    where
        P: HasWindowHandle,
        H: WindowHandler + 'static,
        B: FnOnce(&mut crate::Window) -> H + Send + 'static,
    {
        WindowHandle::X11(crate::x11::Window::open_parented(parent, options, build))
    }

    pub fn open_floating<H, B>(options: WindowOpenOptions, build: B) -> WindowHandle
    where
        H: WindowHandler + 'static,
        B: FnOnce(&mut crate::Window) -> H + Send + 'static,
    {
        #[cfg(feature = "wayland")]
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            return WindowHandle::Wayland(crate::wayland::window::Window::open_floating(
                options, build,
            ));
        }
        WindowHandle::X11(crate::x11::Window::open_floating(options, build))
    }

    pub fn open_blocking<H, B>(options: WindowOpenOptions, build: B)
    where
        H: WindowHandler + 'static,
        B: FnOnce(&mut crate::Window) -> H + Send + 'static,
    {
        crate::x11::Window::open_blocking(options, build)
    }

    pub fn close(&mut self) {
        match self {
            Self::X11(window) => window.close(),
            #[cfg(feature = "wayland")]
            Self::Wayland(window) => window.close(),
        }
    }

    pub fn resize(&mut self, size: Size) {
        match self {
            Self::X11(window) => window.resize(size),
            #[cfg(feature = "wayland")]
            Self::Wayland(window) => window.resize(size),
        }
    }

    pub fn set_mouse_cursor(&mut self, cursor: MouseCursor) {
        match self {
            Self::X11(window) => window.set_mouse_cursor(cursor),
            #[cfg(feature = "wayland")]
            Self::Wayland(window) => window.set_mouse_cursor(cursor),
        }
    }

    pub fn has_focus(&mut self) -> bool {
        match self {
            Self::X11(window) => window.has_focus(),
            #[cfg(feature = "wayland")]
            Self::Wayland(window) => window.has_focus(),
        }
    }

    pub fn focus(&mut self) {
        match self {
            Self::X11(window) => window.focus(),
            #[cfg(feature = "wayland")]
            Self::Wayland(window) => window.focus(),
        }
    }

    #[cfg(feature = "opengl")]
    pub fn gl_context(&self) -> Option<&crate::gl::GlContext> {
        match self {
            Self::X11(window) => window.gl_context(),
            #[cfg(feature = "wayland")]
            Self::Wayland(window) => window.gl_context(),
        }
    }
}

impl HasWindowHandle for Window<'_> {
    fn window_handle(&self) -> Result<RwhWindowHandle<'_>, HandleError> {
        match self {
            Self::X11(window) => window.window_handle(),
            #[cfg(feature = "wayland")]
            Self::Wayland(window) => window.window_handle(),
        }
    }
}

impl HasDisplayHandle for Window<'_> {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        match self {
            Self::X11(window) => window.display_handle(),
            #[cfg(feature = "wayland")]
            Self::Wayland(window) => window.display_handle(),
        }
    }
}

pub fn request_event_loop_stop() {
    crate::x11::request_event_loop_stop();
}
