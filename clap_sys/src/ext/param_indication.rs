use crate::color::clap_color_t;
use crate::id::clap_id;
use crate::plugin::clap_plugin_t;
use std::ffi::c_char;

pub const CLAP_EXT_PARAM_INDICATION: &str = "clap.param-indication/4\0";
pub const CLAP_EXT_PARAM_INDICATION_COMPAT: &str = "clap.param-indication.draft/4\0";

pub const CLAP_PARAM_INDICATION_AUTOMATION_NONE: u32 = 0;
pub const CLAP_PARAM_INDICATION_AUTOMATION_PRESENT: u32 = 1;
pub const CLAP_PARAM_INDICATION_AUTOMATION_PLAYING: u32 = 2;
pub const CLAP_PARAM_INDICATION_AUTOMATION_RECORDING: u32 = 3;
pub const CLAP_PARAM_INDICATION_AUTOMATION_OVERRIDING: u32 = 4;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_param_indication_t {
    pub set_mapping: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            param_id: clap_id,
            has_mapping: bool,
            color: *const clap_color_t,
            label: *const c_char,
            description: *const c_char,
        ),
    >,
    pub set_automation: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            param_id: clap_id,
            automation_state: u32,
            color: *const clap_color_t,
        ),
    >,
}
