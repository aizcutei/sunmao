//! EGL resources owned by one Wayland window thread.
use std::{ffi::c_void, marker::PhantomData, ptr::NonNull};

use khronos_egl as egl;
use wayland_client::{protocol::wl_surface::WlSurface, Connection, Proxy};

use crate::gl::{GlConfig, Profile};

pub(crate) struct Context {
    api: egl::DynamicInstance<egl::EGL1_5>,
    display: egl::Display,
    context: Option<egl::Context>,
    surface: Option<egl::Surface>,
    native: Option<NonNull<wayland_sys::egl::wl_egl_window>>,
    // Keep the C display alive until every EGL resource has been released.
    _connection: Connection,
    _surface: WlSurface,
    _thread: PhantomData<*mut ()>,
}

impl Context {
    /// The surface must already have acknowledged its initial configure.
    pub(crate) fn new(
        connection: &Connection,
        surface: &WlSurface,
        width: i32,
        height: i32,
        config: &GlConfig,
    ) -> Result<Self, String> {
        if width <= 0 || height <= 0 || !surface.is_alive() {
            return Err("invalid Wayland EGL surface or dimensions".into());
        }
        let native_api =
            wayland_sys::egl::wayland_egl_option().ok_or("libwayland-egl is unavailable")?;
        let api = unsafe { egl::DynamicInstance::<egl::EGL1_5>::load_required() }
            .map_err(|e| e.to_string())?;
        // EGL_EXT_platform_wayland specifies 0x31D8. Explicit platform
        // selection avoids guessing X11 from DISPLAY on mixed desktops.
        let display = unsafe {
            api.get_platform_display(
                0x31D8,
                connection.backend().display_ptr().cast(),
                &[egl::ATTRIB_NONE],
            )
        }
        .map_err(|e| e.to_string())?;
        api.initialize(display).map_err(|e| e.to_string())?;
        let mut owner = Self {
            api,
            display,
            context: None,
            surface: None,
            native: None,
            _connection: connection.clone(),
            _surface: surface.clone(),
            _thread: PhantomData,
        };
        // Install the owner before fallible creation so every partial
        // initialization follows the same destruction path.
        owner
            .api
            .bind_api(egl::OPENGL_API)
            .map_err(|e| e.to_string())?;
        let attributes = [
            egl::SURFACE_TYPE,
            egl::WINDOW_BIT,
            egl::RENDERABLE_TYPE,
            egl::OPENGL_BIT,
            egl::RED_SIZE,
            config.red_bits as i32,
            egl::GREEN_SIZE,
            config.green_bits as i32,
            egl::BLUE_SIZE,
            config.blue_bits as i32,
            egl::ALPHA_SIZE,
            config.alpha_bits as i32,
            egl::DEPTH_SIZE,
            config.depth_bits as i32,
            egl::STENCIL_SIZE,
            config.stencil_bits as i32,
            egl::SAMPLE_BUFFERS,
            i32::from(config.samples.is_some()),
            egl::SAMPLES,
            config.samples.unwrap_or(0) as i32,
            egl::NONE,
        ];
        let format = owner
            .api
            .choose_first_config(display, &attributes)
            .map_err(|e| e.to_string())?
            .ok_or("no matching EGL configuration")?;
        let profile = match config.profile {
            Profile::Core => egl::CONTEXT_OPENGL_CORE_PROFILE_BIT,
            Profile::Compatibility => egl::CONTEXT_OPENGL_COMPATIBILITY_PROFILE_BIT,
        };
        owner.context = Some(
            owner
                .api
                .create_context(
                    display,
                    format,
                    None,
                    &[
                        egl::CONTEXT_MAJOR_VERSION,
                        config.version.0 as i32,
                        egl::CONTEXT_MINOR_VERSION,
                        config.version.1 as i32,
                        egl::CONTEXT_OPENGL_PROFILE_MASK,
                        profile,
                        egl::NONE,
                    ],
                )
                .map_err(|e| e.to_string())?,
        );
        owner.native = NonNull::new(unsafe {
            (native_api.wl_egl_window_create)(surface.id().as_ptr(), width, height)
        });
        let native = owner.native.ok_or("wl_egl_window_create failed")?;
        let surface_attributes = if config.srgb {
            vec![egl::GL_COLORSPACE, egl::GL_COLORSPACE_SRGB, egl::NONE]
        } else {
            vec![egl::NONE]
        };
        owner.surface = Some(
            unsafe {
                owner.api.create_window_surface(
                    display,
                    format,
                    native.as_ptr().cast(),
                    Some(&surface_attributes),
                )
            }
            .map_err(|e| e.to_string())?,
        );
        owner.make_current()?;
        owner
            .api
            .swap_interval(display, i32::from(config.vsync))
            .map_err(|e| e.to_string())?;
        owner.make_not_current()?;
        Ok(owner)
    }

    pub(crate) fn make_current(&self) -> Result<(), String> {
        self.api
            .make_current(self.display, self.surface, self.surface, self.context)
            .map_err(|e| e.to_string())
    }

    pub(crate) fn make_not_current(&self) -> Result<(), String> {
        self.api
            .make_current(self.display, None, None, None)
            .map_err(|e| e.to_string())
    }

    pub(crate) fn get_proc_address(&self, symbol: &str) -> *const c_void {
        if symbol.contains('\0') {
            return std::ptr::null();
        }
        self.api
            .get_proc_address(symbol)
            .map_or(std::ptr::null(), |p| p as *const c_void)
    }

    pub(crate) fn swap_buffers(&self) -> Result<(), String> {
        self.api
            .swap_buffers(self.display, self.surface.ok_or("EGL surface missing")?)
            .map_err(|e| e.to_string())
    }

    pub(crate) fn resize(&self, width: i32, height: i32) {
        if width <= 0 || height <= 0 {
            return;
        }
        if let (Some(native), Some(api)) = (self.native, wayland_sys::egl::wayland_egl_option()) {
            unsafe {
                (api.wl_egl_window_resize)(native.as_ptr(), width, height, 0, 0);
            }
        }
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // Do not detach a different window's current context.
        if self.context.is_some() && self.api.get_current_context() == self.context {
            let _ = self.make_not_current();
        }
        if let Some(surface) = self.surface.take() {
            let _ = self.api.destroy_surface(self.display, surface);
        }
        if let Some(context) = self.context.take() {
            let _ = self.api.destroy_context(self.display, context);
        }
        if let (Some(native), Some(api)) =
            (self.native.take(), wayland_sys::egl::wayland_egl_option())
        {
            unsafe {
                (api.wl_egl_window_destroy)(native.as_ptr());
            }
        }
        let _ = self.api.terminate(self.display);
    }
}
