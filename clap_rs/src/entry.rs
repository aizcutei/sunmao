use crate::plugin::Plugin;
use crate::plugin_instance::*;
use crate::ext::gui::GuiHandler;
use clap_sys::plugin::clap_plugin_t;
use clap_sys::plugin::clap_plugin_descriptor_t;
use clap_sys::host::clap_host_t;
use clap_sys::ext::gui::{clap_plugin_gui_t, CLAP_EXT_GUI};
use std::ptr;
use std::ffi::{c_void, c_char, CStr};

pub struct PluginEntry;

impl PluginEntry {
    pub unsafe fn create_plugin<P: Plugin>(
        host: *const clap_host_t,
        descriptor: *const clap_plugin_descriptor_t,
    ) -> *const clap_plugin_t {
        let instance = Box::new(PluginInstance::<P>::new(unsafe { crate::plugin::HostHandle::from_raw(host) }));
        
        let plugin = Box::new(clap_plugin_t {
            desc: descriptor,
            plugin_data: Box::into_raw(instance) as *mut c_void,
            init: Some(plugin_init::<P>),
            destroy: Some(plugin_destroy::<P>),
            activate: Some(plugin_activate::<P>),
            deactivate: Some(plugin_deactivate::<P>),
            start_processing: Some(plugin_start_processing::<P>),
            stop_processing: Some(plugin_stop_processing::<P>),
            reset: Some(plugin_reset::<P>),
            process: Some(plugin_process::<P>),
            get_extension: Some(plugin_get_extension::<P>),
            on_main_thread: Some(plugin_on_main_thread::<P>),
        });
        
        Box::into_raw(plugin)
    }
}

/// Entry point for plugins with GUI support
pub struct PluginEntryWithGui;

impl PluginEntryWithGui {
    pub unsafe fn create_plugin<P: Plugin + GuiHandler>(
        host: *const clap_host_t,
        descriptor: *const clap_plugin_descriptor_t,
    ) -> *const clap_plugin_t {
        let instance = Box::new(PluginInstanceWithGui::<P>::new(unsafe { crate::plugin::HostHandle::from_raw(host) }));
        
        let plugin = Box::new(clap_plugin_t {
            desc: descriptor,
            plugin_data: Box::into_raw(instance) as *mut c_void,
            init: Some(plugin_init_with_gui::<P>),
            destroy: Some(plugin_destroy_with_gui::<P>),
            activate: Some(plugin_activate_with_gui::<P>),
            deactivate: Some(plugin_deactivate_with_gui::<P>),
            start_processing: Some(plugin_start_processing_with_gui::<P>),
            stop_processing: Some(plugin_stop_processing_with_gui::<P>),
            reset: Some(plugin_reset_with_gui::<P>),
            process: Some(plugin_process_with_gui::<P>),
            get_extension: Some(plugin_get_extension_with_gui::<P>),
            on_main_thread: Some(plugin_on_main_thread_with_gui::<P>),
        });
        
        Box::into_raw(plugin)
    }
}
