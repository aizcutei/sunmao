use crate::Size;

/// The dpi scaling policy of the window
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowScalePolicy {
    /// Use the system's dpi scale factor
    SystemScaleFactor,
    /// Use the given dpi scale factor (e.g. `1.0` = 96 dpi)
    ScaleFactor(f64),
}

/// The options for opening a new window
pub struct WindowOpenOptions {
    pub title: String,

    /// The logical size of the window.
    ///
    /// These dimensions will be scaled by the scaling policy specified in `scale`. Mouse
    /// position will be passed back as logical coordinates.
    pub size: Size,

    /// The dpi scaling policy
    pub scale: WindowScalePolicy,

    /// If provided, then an OpenGL context will be created for this window. You'll be able to
    /// access this context through [crate::Window::gl_context].
    #[cfg(feature = "opengl")]
    pub gl_config: Option<crate::gl::GlConfig>,

    /// If `true`, a `CAMetalLayer` will be created on the window's NSView,
    /// enabling wgpu/Metal rendering. Access via [crate::Window::metal_layer].
    #[cfg(feature = "metal")]
    pub metal_layer: bool,
}

impl WindowOpenOptions {
    /// Create window options with all feature-dependent facilities disabled.
    ///
    /// Constructing this inside `baseview` keeps downstream crates independent
    /// from Cargo feature unification changing this struct's optional fields.
    pub fn new(title: impl Into<String>, size: Size, scale: WindowScalePolicy) -> Self {
        Self {
            title: title.into(),
            size,
            scale,
            #[cfg(feature = "opengl")]
            gl_config: None,
            #[cfg(feature = "metal")]
            metal_layer: false,
        }
    }
}
