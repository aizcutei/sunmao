use crate::host::clap_host_t;
use crate::plugin::clap_plugin_t;

pub const CLAP_EXT_AMBISONIC: &str = "clap.ambisonic/3\0";
pub const CLAP_EXT_AMBISONIC_COMPAT: &str = "clap.ambisonic.draft/3\0";
pub const CLAP_PORT_AMBISONIC: &str = "ambisonic\0";

pub const CLAP_AMBISONIC_ORDERING_FUMA: u32 = 0;
pub const CLAP_AMBISONIC_ORDERING_ACN: u32 = 1;

pub const CLAP_AMBISONIC_NORMALIZATION_MAXN: u32 = 0;
pub const CLAP_AMBISONIC_NORMALIZATION_SN3D: u32 = 1;
pub const CLAP_AMBISONIC_NORMALIZATION_N3D: u32 = 2;
pub const CLAP_AMBISONIC_NORMALIZATION_SN2D: u32 = 3;
pub const CLAP_AMBISONIC_NORMALIZATION_N2D: u32 = 4;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_ambisonic_config_t {
    pub ordering: u32,
    pub normalization: u32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_ambisonic_t {
    pub is_config_supported: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            config: *const clap_ambisonic_config_t,
        ) -> bool,
    >,
    pub get_config: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            is_input: bool,
            port_index: u32,
            config: *mut clap_ambisonic_config_t,
        ) -> bool,
    >,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_ambisonic_t {
    pub changed: Option<unsafe extern "C" fn(host: *const clap_host_t)>,
}
