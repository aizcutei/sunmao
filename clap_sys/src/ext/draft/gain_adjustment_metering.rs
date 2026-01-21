use crate::plugin::clap_plugin_t;

pub const CLAP_EXT_GAIN_ADJUSTMENT_METERING: &str = "clap.gain-adjustment-metering/0\0";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_gain_adjustment_metering_t {
    pub get: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t) -> f64>,
}
