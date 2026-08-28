use std::ffi::CStr;
use std::ptr;
use std::sync::Arc;

use clap_rs::clap_sys::entry::clap_plugin_entry_t;
use clap_rs::clap_sys::factory::plugin_factory::{clap_plugin_factory_t, CLAP_PLUGIN_FACTORY_ID};
use sunmao_backend_clap::export_sunmao_clap_plugin;
use sunmao_core::{
    derive_clap_id, AudioBuffer, EventQueue, Params, ProcessContext, ProcessStatus, SunmaoPlugin,
};

#[derive(Default)]
struct EmptyParams;

impl Params for EmptyParams {
    fn get_normalized(&self, _id: &str) -> Option<f32> {
        None
    }

    fn set_normalized(&self, _id: &str, _value: f32) {}

    fn descriptors(&self) -> Vec<sunmao_core::ParamDescriptor> {
        Vec::new()
    }
}

#[derive(Default)]
struct DefaultSynthMetadataPlugin;

impl SunmaoPlugin for DefaultSynthMetadataPlugin {
    const NAME: &'static str = "Default Metadata Synth";
    const VENDOR: &'static str = "SunMao Test";
    const URL: &'static str = "https://example.invalid";
    type Params = EmptyParams;

    fn input_channels(&self) -> u32 {
        0
    }

    fn accepts_midi(&self) -> bool {
        true
    }

    fn params(&self) -> Arc<Self::Params> {
        Arc::new(EmptyParams)
    }

    fn process(
        &mut self,
        _buffer: &mut AudioBuffer,
        _events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        ProcessStatus::Normal
    }
}

export_sunmao_clap_plugin!(DefaultSynthMetadataPlugin);

unsafe extern "C" {
    static clap_entry: clap_plugin_entry_t;
}

#[test]
fn default_synth_export_has_a_plugin_specific_id_and_synth_features() {
    let entry = unsafe { &clap_entry };
    assert!(unsafe { (entry.init.expect("entry init"))(ptr::null()) });
    let factory = unsafe {
        entry.get_factory.expect("factory callback")(CLAP_PLUGIN_FACTORY_ID.as_ptr().cast())
    }
    .cast::<clap_plugin_factory_t>();
    assert!(!factory.is_null());

    let descriptor = unsafe {
        ((*factory).get_plugin_descriptor.expect("descriptor"))(factory, 0)
            .as_ref()
            .expect("default descriptor")
    };
    let id = unsafe { CStr::from_ptr(descriptor.id) }
        .to_str()
        .expect("UTF-8 CLAP id");
    assert_eq!(
        id,
        derive_clap_id(
            DefaultSynthMetadataPlugin::VENDOR,
            DefaultSynthMetadataPlugin::NAME
        )
    );

    let first_feature = unsafe { *descriptor.features };
    let second_feature = unsafe { *descriptor.features.add(1) };
    assert_eq!(
        unsafe { CStr::from_ptr(first_feature) }.to_bytes(),
        b"instrument"
    );
    assert_eq!(
        unsafe { CStr::from_ptr(second_feature) }.to_bytes(),
        b"synthesizer"
    );
    assert!(unsafe { *descriptor.features.add(2) }.is_null());
}
