use crate::plugin::clap_plugin_t;

pub const CLAP_EXT_AUDIO_PORTS_ACTIVATION: &str = "clap.audio-ports-activation/2\0";
pub const CLAP_EXT_AUDIO_PORTS_ACTIVATION_COMPAT: &str = "clap.audio-ports-activation/draft-2\0";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_audio_ports_activation_t {
    pub can_activate_while_processing:
        Option<unsafe extern "C" fn(plugin: *const clap_plugin_t) -> bool>,
    pub set_active: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            is_input: bool,
            port_index: u32,
            is_active: bool,
            sample_size: u32,
        ) -> bool,
    >,
}
