use crate::host::clap_host_t;
use crate::plugin::clap_plugin_t;
use std::ffi::{c_char, c_void};

pub const CLAP_EXT_GUI: &str = "clap.gui\0";

/// Uses physical size. Embed with `SetParent`.
pub const CLAP_WINDOW_API_WIN32: &str = "win32\0";
/// Uses **logical** size — upstream says the host should *not* call
/// `clap_plugin_gui->set_scale()` for this API.
pub const CLAP_WINDOW_API_COCOA: &str = "cocoa\0";
/// Uses **logical** size — same `set_scale` caveat as cocoa.
pub const CLAP_WINDOW_API_UIKIT: &str = "uikit\0";
/// Uses physical size. Embed via the XEmbed protocol.
pub const CLAP_WINDOW_API_X11: &str = "x11\0";
/// Uses physical size. Upstream: *"embed is currently not supported, use
/// floating windows"* — there is no way to embed a plugin editor in a host
/// window on Wayland, by the spec, not by omission.
pub const CLAP_WINDOW_API_WAYLAND: &str = "wayland\0";

pub type clap_hwnd = *mut c_void;
pub type clap_nsview = *mut c_void;
pub type clap_uiview = *mut c_void;
pub type clap_xwnd = std::os::raw::c_ulong;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct clap_window_t {
    pub api: *const c_char,
    pub handle: clap_window_handle_u,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union clap_window_handle_u {
    pub cocoa: clap_nsview,
    pub uikit: clap_uiview,
    pub x11: clap_xwnd,
    pub win32: clap_hwnd,
    /// For anything defined outside of CLAP.
    pub ptr: *mut c_void,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_gui_resize_hints_t {
    pub can_resize_horizontally: bool,
    pub can_resize_vertically: bool,
    pub preserve_aspect_ratio: bool,
    pub aspect_ratio_width: u32,
    pub aspect_ratio_height: u32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_gui_t {
    pub is_api_supported: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            api: *const c_char,
            is_floating: bool,
        ) -> bool,
    >,
    pub get_preferred_api: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            api: *mut *const c_char,
            is_floating: *mut bool,
        ) -> bool,
    >,
    pub create: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            api: *const c_char,
            is_floating: bool,
        ) -> bool,
    >,
    pub destroy: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t)>,
    pub set_scale: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, scale: f64) -> bool>,
    pub get_size: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            width: *mut u32,
            height: *mut u32,
        ) -> bool,
    >,
    pub can_resize: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t) -> bool>,
    pub get_resize_hints: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            hints: *mut clap_gui_resize_hints_t,
        ) -> bool,
    >,
    pub adjust_size: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            width: *mut u32,
            height: *mut u32,
        ) -> bool,
    >,
    pub set_size:
        Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, width: u32, height: u32) -> bool>,
    pub set_parent: Option<
        unsafe extern "C" fn(plugin: *const clap_plugin_t, window: *const clap_window_t) -> bool,
    >,
    pub set_transient: Option<
        unsafe extern "C" fn(plugin: *const clap_plugin_t, window: *const clap_window_t) -> bool,
    >,
    pub suggest_title:
        Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, title: *const c_char)>,
    pub show: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t) -> bool>,
    pub hide: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t) -> bool>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_gui_t {
    pub resize_hints_changed: Option<unsafe extern "C" fn(host: *const clap_host_t)>,
    pub request_resize:
        Option<unsafe extern "C" fn(host: *const clap_host_t, width: u32, height: u32) -> bool>,
    pub request_show: Option<unsafe extern "C" fn(host: *const clap_host_t) -> bool>,
    pub request_hide: Option<unsafe extern "C" fn(host: *const clap_host_t) -> bool>,
    pub closed: Option<unsafe extern "C" fn(host: *const clap_host_t, was_destroyed: bool)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, size_of};

    /// Adding the `uikit` member must not move anything: this union crosses the
    /// ABI boundary, so a size or alignment change would silently corrupt every
    /// window handle a host passes in. All members are pointer-sized or a
    /// `c_ulong`, so the union stays exactly one word.
    #[test]
    fn the_window_handle_union_is_one_pointer_wide() {
        assert_eq!(size_of::<clap_window_handle_u>(), size_of::<*mut c_void>());
        assert_eq!(
            align_of::<clap_window_handle_u>(),
            align_of::<*mut c_void>()
        );
        // `clap_window` is the api string plus that union, with no padding
        // beyond alignment.
        assert_eq!(size_of::<clap_window_t>(), 2 * size_of::<*mut c_void>());
    }

    /// The API names are what a host string-compares against. A typo here makes
    /// the plugin silently unsupported rather than failing loudly, so pin the
    /// literals from upstream `clap/ext/gui.h`.
    #[test]
    fn the_window_api_names_match_upstream() {
        assert_eq!(CLAP_WINDOW_API_WIN32, "win32\0");
        assert_eq!(CLAP_WINDOW_API_COCOA, "cocoa\0");
        assert_eq!(CLAP_WINDOW_API_UIKIT, "uikit\0");
        assert_eq!(CLAP_WINDOW_API_X11, "x11\0");
        assert_eq!(CLAP_WINDOW_API_WAYLAND, "wayland\0");
        assert_eq!(CLAP_EXT_GUI, "clap.gui\0");
    }
}
