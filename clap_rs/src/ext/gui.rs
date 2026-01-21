//! GUI Extension for clap_rs
//! 
//! Provides plugin UI embedding in host windows.

use crate::plugin::Plugin;
use crate::plugin_instance::PluginInstanceWithGui;
use clap_sys::plugin::clap_plugin_t;
use clap_sys::ext::gui::{
    clap_plugin_gui_t, clap_window_t, clap_gui_resize_hints_t,
    CLAP_EXT_GUI, CLAP_WINDOW_API_COCOA, CLAP_WINDOW_API_WIN32, CLAP_WINDOW_API_X11
};
use std::ffi::{c_char, c_void, CStr};
use std::ptr;

/// Supported GUI API types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuiApi {
    Cocoa,
    Win32,
    X11,
    Wayland,
}

impl GuiApi {
    pub fn as_cstr(&self) -> &'static str {
        match self {
            GuiApi::Cocoa => "cocoa\0",
            GuiApi::Win32 => "win32\0", 
            GuiApi::X11 => "x11\0",
            GuiApi::Wayland => "wayland\0",
        }
    }
    
    pub fn from_cstr(s: &CStr) -> Option<Self> {
        let bytes = s.to_bytes_with_nul();
        if bytes == b"cocoa\0" { Some(GuiApi::Cocoa) }
        else if bytes == b"win32\0" { Some(GuiApi::Win32) }
        else if bytes == b"x11\0" { Some(GuiApi::X11) }
        else if bytes == b"wayland\0" { Some(GuiApi::Wayland) }
        else { None }
    }
}

/// GUI resize hints
#[derive(Debug, Clone, Copy)]
pub struct GuiResizeHints {
    pub can_resize_horizontally: bool,
    pub can_resize_vertically: bool,
    pub preserve_aspect_ratio: bool,
    pub aspect_ratio_width: u32,
    pub aspect_ratio_height: u32,
}

impl Default for GuiResizeHints {
    fn default() -> Self {
        Self {
            can_resize_horizontally: false,
            can_resize_vertically: false,
            preserve_aspect_ratio: false,
            aspect_ratio_width: 1,
            aspect_ratio_height: 1,
        }
    }
}

/// GUI handler trait for plugins with UI
pub trait GuiHandler {
    /// Check if the given API is supported
    fn is_api_supported(&self, api: GuiApi, is_floating: bool) -> bool { false }
    
    /// Get preferred API (return None to not support GUI)
    fn preferred_api(&self) -> Option<(GuiApi, bool)> { None }
    
    /// Create the GUI (called by host)
    fn gui_create(&mut self, api: GuiApi, is_floating: bool) -> bool { false }
    
    /// Destroy the GUI
    fn gui_destroy(&mut self) {}
    
    /// Set scale factor
    fn gui_set_scale(&mut self, scale: f64) -> bool { true }
    
    /// Get current GUI size
    fn gui_get_size(&self) -> Option<(u32, u32)> { None }
    
    /// Check if GUI can be resized
    fn gui_can_resize(&self) -> bool { false }
    
    /// Get resize hints
    fn gui_get_resize_hints(&self) -> GuiResizeHints { GuiResizeHints::default() }
    
    /// Adjust size to valid dimensions
    fn gui_adjust_size(&self, width: u32, height: u32) -> (u32, u32) { (width, height) }
    
    /// Set GUI size
    fn gui_set_size(&mut self, width: u32, height: u32) -> bool { false }
    
    /// Set parent window
    fn gui_set_parent(&mut self, window: *mut c_void) -> bool { false }
    
    /// Set transient window
    fn gui_set_transient(&mut self, window: *mut c_void) -> bool { false }
    
    /// Show the GUI
    fn gui_show(&mut self) -> bool { false }
    
    /// Hide the GUI
    fn gui_hide(&mut self) -> bool { false }
}

// ======= Callbacks =======

pub(crate) unsafe extern "C" fn gui_is_api_supported<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    api: *const c_char,
    is_floating: bool,
) -> bool {
    if api.is_null() { return false; }
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstanceWithGui<P>) };
    let api_cstr = unsafe { CStr::from_ptr(api) };
    if let Some(gui_api) = GuiApi::from_cstr(api_cstr) {
        instance.plugin.is_api_supported(gui_api, is_floating)
    } else {
        false
    }
}

pub(crate) unsafe extern "C" fn gui_get_preferred_api<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    api: *mut *const c_char,
    is_floating: *mut bool,
) -> bool {
    if api.is_null() || is_floating.is_null() { return false; }
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstanceWithGui<P>) };
    if let Some((pref_api, floating)) = instance.plugin.preferred_api() {
        unsafe {
            *api = pref_api.as_cstr().as_ptr() as *const c_char;
            *is_floating = floating;
        }
        true
    } else {
        false
    }
}

pub(crate) unsafe extern "C" fn gui_create<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    api: *const c_char,
    is_floating: bool,
) -> bool {
    if api.is_null() { return false; }
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstanceWithGui<P>) };
    let api_cstr = unsafe { CStr::from_ptr(api) };
    if let Some(gui_api) = GuiApi::from_cstr(api_cstr) {
        instance.plugin.gui_create(gui_api, is_floating)
    } else {
        false
    }
}

pub(crate) unsafe extern "C" fn gui_destroy<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
) {
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstanceWithGui<P>) };
    instance.plugin.gui_destroy();
}

pub(crate) unsafe extern "C" fn gui_set_scale<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    scale: f64,
) -> bool {
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstanceWithGui<P>) };
    instance.plugin.gui_set_scale(scale)
}

pub(crate) unsafe extern "C" fn gui_get_size<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    width: *mut u32,
    height: *mut u32,
) -> bool {
    if width.is_null() || height.is_null() { return false; }
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstanceWithGui<P>) };
    if let Some((w, h)) = instance.plugin.gui_get_size() {
        unsafe {
            *width = w;
            *height = h;
        }
        true
    } else {
        false
    }
}

pub(crate) unsafe extern "C" fn gui_can_resize<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
) -> bool {
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstanceWithGui<P>) };
    instance.plugin.gui_can_resize()
}

pub(crate) unsafe extern "C" fn gui_get_resize_hints<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    hints: *mut clap_gui_resize_hints_t,
) -> bool {
    if hints.is_null() { return false; }
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstanceWithGui<P>) };
    let h = instance.plugin.gui_get_resize_hints();
    let out = unsafe { &mut *hints };
    out.can_resize_horizontally = h.can_resize_horizontally;
    out.can_resize_vertically = h.can_resize_vertically;
    out.preserve_aspect_ratio = h.preserve_aspect_ratio;
    out.aspect_ratio_width = h.aspect_ratio_width;
    out.aspect_ratio_height = h.aspect_ratio_height;
    true
}

pub(crate) unsafe extern "C" fn gui_adjust_size<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    width: *mut u32,
    height: *mut u32,
) -> bool {
    if width.is_null() || height.is_null() { return false; }
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstanceWithGui<P>) };
    let (w, h) = instance.plugin.gui_adjust_size(unsafe { *width }, unsafe { *height });
    unsafe {
        *width = w;
        *height = h;
    }
    true
}

pub(crate) unsafe extern "C" fn gui_set_size<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    width: u32,
    height: u32,
) -> bool {
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstanceWithGui<P>) };
    instance.plugin.gui_set_size(width, height)
}

pub(crate) unsafe extern "C" fn gui_set_parent<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    window: *const clap_window_t,
) -> bool {
    if window.is_null() { return false; }
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstanceWithGui<P>) };
    let handle = unsafe { (*window).handle.ptr };
    instance.plugin.gui_set_parent(handle)
}

pub(crate) unsafe extern "C" fn gui_set_transient<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    window: *const clap_window_t,
) -> bool {
    if window.is_null() { return false; }
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstanceWithGui<P>) };
    let handle = unsafe { (*window).handle.ptr };
    instance.plugin.gui_set_transient(handle)
}

pub(crate) unsafe extern "C" fn gui_suggest_title<P: Plugin + GuiHandler>(
    _plugin: *const clap_plugin_t,
    _title: *const c_char,
) {
    // Optional: plugins can ignore this
}

pub(crate) unsafe extern "C" fn gui_show<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
) -> bool {
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstanceWithGui<P>) };
    instance.plugin.gui_show()
}

pub(crate) unsafe extern "C" fn gui_hide<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
) -> bool {
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstanceWithGui<P>) };
    instance.plugin.gui_hide()
}

/// Create GUI extension struct (only for plugins implementing GuiHandler)
pub fn create_gui_ext<P: Plugin + GuiHandler>() -> clap_plugin_gui_t {
    clap_plugin_gui_t {
        is_api_supported: Some(gui_is_api_supported::<P>),
        get_preferred_api: Some(gui_get_preferred_api::<P>),
        create: Some(gui_create::<P>),
        destroy: Some(gui_destroy::<P>),
        set_scale: Some(gui_set_scale::<P>),
        get_size: Some(gui_get_size::<P>),
        can_resize: Some(gui_can_resize::<P>),
        get_resize_hints: Some(gui_get_resize_hints::<P>),
        adjust_size: Some(gui_adjust_size::<P>),
        set_size: Some(gui_set_size::<P>),
        set_parent: Some(gui_set_parent::<P>),
        set_transient: Some(gui_set_transient::<P>),
        suggest_title: Some(gui_suggest_title::<P>),
        show: Some(gui_show::<P>),
        hide: Some(gui_hide::<P>),
    }
}


