use std::ptr;
use std::sync::Arc;

use clap_rs::clap_sys::entry::clap_plugin_entry_t;
use clap_rs::clap_sys::factory::plugin_factory::clap_plugin_factory_t;
use sunmao_backend_clap::export_sunmao_clap_plugin;
use sunmao_core::{
    AudioBuffer, ClapInfo, EventQueue, Params, ProcessContext, ProcessStatus, SunmaoPlugin,
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
struct InvalidMetadataPlugin;

impl SunmaoPlugin for InvalidMetadataPlugin {
    const NAME: &'static str = "Metadata test";
    const VENDOR: &'static str = "SunMao test";
    const URL: &'static str = "https://example.invalid";
    type Params = EmptyParams;

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

    fn clap_info() -> ClapInfo {
        ClapInfo {
            id: "com.sunmao.invalid\0id",
            features: &[],
        }
    }
}

export_sunmao_clap_plugin!(InvalidMetadataPlugin);

// The export macro intentionally keeps the ABI symbol stable. Referencing it
// through an extern declaration also exercises the same path a host uses.
unsafe extern "C" {
    static clap_entry: clap_plugin_entry_t;
}

#[test]
fn invalid_metadata_fails_entry_without_unwinding() {
    let entry = unsafe { &clap_entry };
    assert!(!unsafe { (entry.init.expect("entry init"))(ptr::null()) });

    let factory = unsafe {
        entry.get_factory.expect("factory callback")(
            clap_rs::clap_sys::factory::plugin_factory::CLAP_PLUGIN_FACTORY_ID
                .as_ptr()
                .cast(),
        )
    };
    assert!(!factory.is_null());
    let factory = factory.cast::<clap_plugin_factory_t>();
    assert_eq!(
        unsafe { ((*factory).get_plugin_count.expect("count"))(factory) },
        0
    );
    assert!(unsafe {
        ((*factory).get_plugin_descriptor.expect("descriptor"))(factory, 0).is_null()
    });
}
