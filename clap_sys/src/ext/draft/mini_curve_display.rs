use crate::host::clap_host_t;
use crate::plugin::clap_plugin_t;
use std::ffi::c_char;

pub const CLAP_EXT_MINI_CURVE_DISPLAY: &str = "clap.mini-curve-display/3\0";

pub const CLAP_MINI_CURVE_DISPLAY_CURVE_KIND_UNSPECIFIED: i32 = 0;
pub const CLAP_MINI_CURVE_DISPLAY_CURVE_KIND_GAIN_RESPONSE: i32 = 1;
pub const CLAP_MINI_CURVE_DISPLAY_CURVE_KIND_PHASE_RESPONSE: i32 = 2;
pub const CLAP_MINI_CURVE_DISPLAY_CURVE_KIND_TRANSFER_CURVE: i32 = 3;
pub const CLAP_MINI_CURVE_DISPLAY_CURVE_KIND_GAIN_REDUCTION: i32 = 4;
pub const CLAP_MINI_CURVE_DISPLAY_CURVE_KIND_TIME_SERIES: i32 = 5;

pub type clap_mini_curve_display_curve_kind = i32;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_mini_curve_display_curve_hints_t {
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_mini_curve_display_curve_data_t {
    pub curve_kind: i32,
    pub values: *mut u16,
    pub values_count: u32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_mini_curve_display_t {
    pub get_curve_count: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t) -> u32>,
    pub render: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            curves: *mut clap_mini_curve_display_curve_data_t,
            curves_size: u32,
        ) -> u32,
    >,
    pub set_observed: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, is_observed: bool)>,
    pub get_axis_name: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            curve_index: u32,
            x_name: *mut c_char,
            y_name: *mut c_char,
            name_capacity: u32,
        ) -> bool,
    >,
}

pub const CLAP_MINI_CURVE_DISPLAY_CURVE_CHANGED: u32 = 1 << 0;
pub const CLAP_MINI_CURVE_DISPLAY_AXIS_NAME_CHANGED: u32 = 1 << 1;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_mini_curve_display_t {
    pub get_hints: Option<
        unsafe extern "C" fn(
            host: *const clap_host_t,
            kind: u32,
            hints: *mut clap_mini_curve_display_curve_hints_t,
        ) -> bool,
    >,
    pub set_dynamic: Option<unsafe extern "C" fn(host: *const clap_host_t, is_dynamic: bool)>,
    pub changed: Option<unsafe extern "C" fn(host: *const clap_host_t, flags: u32)>,
}
