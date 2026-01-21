#[macro_export]
macro_rules! export_au_component {
    ($factory_fn:ident, $plugin:ty, $descriptor:expr) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $factory_fn(
            in_desc: *const $crate::AudioComponentDescription,
        ) -> *mut $crate::AudioComponentPlugInInterface {
            static FACTORY: std::sync::OnceLock<$crate::AuFactory> = std::sync::OnceLock::new();
            let factory = FACTORY.get_or_init(|| $crate::AuFactory {
                descriptor: $descriptor,
                create: |sample_rate, max_frames| {
                    Box::new(<$plugin as $crate::AuPlugin>::new(sample_rate, max_frames))
                },
            });
            $crate::set_factory(factory);
            unsafe { $crate::au_component_factory(in_desc) }
        }
    };
}
