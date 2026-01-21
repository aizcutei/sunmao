use crate::process::ProcessContext;
use crate::ext::{AudioPortInfo, NotePortInfo, ParameterInfo, VoiceInfo, RenderMode};
use clap_sys::host::clap_host_t;

// Re-export commonly used types
pub use clap_sys::version::CLAP_VERSION;
pub use clap_sys::process::CLAP_PROCESS_CONTINUE;

pub struct PluginInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub vendor: &'static str,
    pub url: &'static str,
    pub manual_url: &'static str,
    pub support_url: &'static str,
    pub version: &'static str,
    pub description: &'static str,
}

pub trait Plugin: Sized {
    type AudioProcessor;
    
    fn new(host: HostHandle) -> Self;
    
    // Lifecycle
    fn init(&mut self) -> bool { true }
    fn activate(&mut self, _sample_rate: f64, _min_frames: u32, _max_frames: u32) -> bool { true }
    fn deactivate(&mut self) {}
    fn reset(&mut self) {}
    
    // Processing
    fn process(&mut self, process: ProcessContext) -> clap_sys::process::clap_process_status;

    // Parameters
    fn declare_parameters(&self) -> Vec<ParameterInfo> { Vec::new() }
    fn get_parameter(&self, id: u32) -> f64;
    fn set_parameter(&mut self, id: u32, value: f64);
    
    // Ports
    fn audio_ports_config(&self) -> Vec<AudioPortInfo> { Vec::new() }
    fn note_ports_config(&self) -> Vec<NotePortInfo> { Vec::new() }
    
    // Latency (samples of processing delay)
    fn latency(&self) -> u32 { 0 }
    
    // Tail (samples of tail after input stops, e.g., reverb)
    fn tail(&self) -> u32 { 0 }
    
    // Voice info for polyphonic synths
    fn voice_info(&self) -> Option<VoiceInfo> { None }
    
    // Render mode (realtime vs offline)
    fn has_hard_realtime_requirement(&self) -> bool { false }
    fn set_render_mode(&mut self, _mode: RenderMode) -> bool { true }
}

#[derive(Clone)]
pub struct HostHandle {
    raw: *const clap_host_t,
}

impl HostHandle {
    pub unsafe fn from_raw(raw: *const clap_host_t) -> Self {
        Self { raw }
    }
}

unsafe impl Send for HostHandle {}
unsafe impl Sync for HostHandle {}

// C-ABI shim (boilerplate to forward C calls to Rust Trait)
// This is usually hidden behind a macro
#[macro_export]
macro_rules! export_clap_plugin {
    ($plugin_type:ty, $plugin_info:expr, $features:expr) => {
        mod __clap_rs_impl {
            use super::*;
            use std::ffi::{c_void, CStr, c_char};
            use $crate::CLAP_VERSION;
            
            // Sync Wrappers
            #[repr(transparent)]
            struct SyncDescriptor($crate::clap_sys::plugin::clap_plugin_descriptor_t);
            unsafe impl Sync for SyncDescriptor {}
            
            #[repr(transparent)]
            struct SyncFactory($crate::clap_sys::factory::plugin_factory::clap_plugin_factory_t);
            unsafe impl Sync for SyncFactory {}

            // Static Descriptor
            static DESCRIPTOR: SyncDescriptor = SyncDescriptor($crate::clap_sys::plugin::clap_plugin_descriptor_t {
                clap_version: $crate::CLAP_VERSION,
                id: $plugin_info.id.as_ptr() as *const c_char,
                name: $plugin_info.name.as_ptr() as *const c_char,
                vendor: $plugin_info.vendor.as_ptr() as *const c_char,
                url: $plugin_info.url.as_ptr() as *const c_char,
                manual_url: $plugin_info.manual_url.as_ptr() as *const c_char,
                support_url: $plugin_info.support_url.as_ptr() as *const c_char,
                version: $plugin_info.version.as_ptr() as *const c_char,
                description: $plugin_info.description.as_ptr() as *const c_char,
                features: $features.as_ptr(), 
            });

            // Factory
            unsafe extern "C" fn create_plugin(
                factory: *const $crate::clap_sys::factory::plugin_factory::clap_plugin_factory_t,
                host: *const $crate::clap_sys::host::clap_host_t,
                plugin_id: *const c_char,
            ) -> *const $crate::clap_sys::plugin::clap_plugin_t {
                if plugin_id.is_null() {
                    return std::ptr::null();
                }
                let id = CStr::from_ptr(plugin_id);
                let target_id = CStr::from_ptr(DESCRIPTOR.0.id);
                
                if id == target_id {
                    return $crate::entry::PluginEntry::create_plugin::<$plugin_type>(host, &DESCRIPTOR.0);
                }
                std::ptr::null()
            }
            
            static FACTORY: SyncFactory = SyncFactory($crate::clap_sys::factory::plugin_factory::clap_plugin_factory_t {
                get_plugin_count: Some(get_plugin_count),
                get_plugin_descriptor: Some(get_plugin_descriptor),
                create_plugin: Some(create_plugin),
            });

            unsafe extern "C" fn get_plugin_count(_factory: *const $crate::clap_sys::factory::plugin_factory::clap_plugin_factory_t) -> u32 { 1 }
            unsafe extern "C" fn get_plugin_descriptor(_factory: *const $crate::clap_sys::factory::plugin_factory::clap_plugin_factory_t, index: u32) -> *const $crate::clap_sys::plugin::clap_plugin_descriptor_t {
                if index == 0 {
                    &DESCRIPTOR.0
                } else {
                    std::ptr::null()
                }
            }

            // Entry Point
            unsafe extern "C" fn entry_init(_path: *const c_char) -> bool { true }
            unsafe extern "C" fn entry_deinit() {}
            unsafe extern "C" fn entry_get_factory(factory_id: *const c_char) -> *const c_void {
                 if factory_id.is_null() { return std::ptr::null(); }
                 let id = CStr::from_ptr(factory_id);
                 let expected = CStr::from_ptr($crate::clap_sys::factory::plugin_factory::CLAP_PLUGIN_FACTORY_ID.as_ptr() as *const c_char);
                 if id == expected {
                     return &FACTORY.0 as *const _ as *const c_void;
                 }
                 std::ptr::null()
            }

            #[unsafe(no_mangle)]
            pub static clap_entry: $crate::clap_sys::entry::clap_plugin_entry_t = $crate::clap_sys::entry::clap_plugin_entry_t {
                clap_version: $crate::CLAP_VERSION,
                init: Some(entry_init),
                deinit: Some(entry_deinit),
                get_factory: Some(entry_get_factory),
            };
        }
    };
}

/// Export macro for plugins with GUI support
/// Requires the plugin to implement both Plugin and GuiHandler traits
#[macro_export]
macro_rules! export_clap_plugin_with_gui {
    ($plugin_type:ty, $plugin_info:expr, $features:expr) => {
        mod __clap_rs_impl {
            use super::*;
            use std::ffi::{c_void, CStr, c_char};
            use $crate::CLAP_VERSION;
            
            // Sync Wrappers
            #[repr(transparent)]
            struct SyncDescriptor($crate::clap_sys::plugin::clap_plugin_descriptor_t);
            unsafe impl Sync for SyncDescriptor {}
            
            #[repr(transparent)]
            struct SyncFactory($crate::clap_sys::factory::plugin_factory::clap_plugin_factory_t);
            unsafe impl Sync for SyncFactory {}

            // Static Descriptor
            static DESCRIPTOR: SyncDescriptor = SyncDescriptor($crate::clap_sys::plugin::clap_plugin_descriptor_t {
                clap_version: $crate::CLAP_VERSION,
                id: $plugin_info.id.as_ptr() as *const c_char,
                name: $plugin_info.name.as_ptr() as *const c_char,
                vendor: $plugin_info.vendor.as_ptr() as *const c_char,
                url: $plugin_info.url.as_ptr() as *const c_char,
                manual_url: $plugin_info.manual_url.as_ptr() as *const c_char,
                support_url: $plugin_info.support_url.as_ptr() as *const c_char,
                version: $plugin_info.version.as_ptr() as *const c_char,
                description: $plugin_info.description.as_ptr() as *const c_char,
                features: $features.as_ptr(), 
            });

            // Factory
            unsafe extern "C" fn create_plugin(
                _factory: *const $crate::clap_sys::factory::plugin_factory::clap_plugin_factory_t,
                host: *const $crate::clap_sys::host::clap_host_t,
                plugin_id: *const c_char,
            ) -> *const $crate::clap_sys::plugin::clap_plugin_t {
                if plugin_id.is_null() {
                    return std::ptr::null();
                }
                let id = CStr::from_ptr(plugin_id);
                let target_id = CStr::from_ptr(DESCRIPTOR.0.id);
                
                if id == target_id {
                    return $crate::entry::PluginEntryWithGui::create_plugin::<$plugin_type>(host, &DESCRIPTOR.0);
                }
                std::ptr::null()
            }
            
            static FACTORY: SyncFactory = SyncFactory($crate::clap_sys::factory::plugin_factory::clap_plugin_factory_t {
                get_plugin_count: Some(get_plugin_count),
                get_plugin_descriptor: Some(get_plugin_descriptor),
                create_plugin: Some(create_plugin),
            });

            unsafe extern "C" fn get_plugin_count(_factory: *const $crate::clap_sys::factory::plugin_factory::clap_plugin_factory_t) -> u32 { 1 }
            unsafe extern "C" fn get_plugin_descriptor(_factory: *const $crate::clap_sys::factory::plugin_factory::clap_plugin_factory_t, index: u32) -> *const $crate::clap_sys::plugin::clap_plugin_descriptor_t {
                if index == 0 {
                    &DESCRIPTOR.0
                } else {
                    std::ptr::null()
                }
            }

            // Entry Point
            unsafe extern "C" fn entry_init(_path: *const c_char) -> bool { true }
            unsafe extern "C" fn entry_deinit() {}
            unsafe extern "C" fn entry_get_factory(factory_id: *const c_char) -> *const c_void {
                 if factory_id.is_null() { return std::ptr::null(); }
                 let id = CStr::from_ptr(factory_id);
                 let expected = CStr::from_ptr($crate::clap_sys::factory::plugin_factory::CLAP_PLUGIN_FACTORY_ID.as_ptr() as *const c_char);
                 if id == expected {
                     return &FACTORY.0 as *const _ as *const c_void;
                 }
                 std::ptr::null()
            }

            #[unsafe(no_mangle)]
            pub static clap_entry: $crate::clap_sys::entry::clap_plugin_entry_t = $crate::clap_sys::entry::clap_plugin_entry_t {
                clap_version: $crate::CLAP_VERSION,
                init: Some(entry_init),
                deinit: Some(entry_deinit),
                get_factory: Some(entry_get_factory),
            };
        }
    };
}
