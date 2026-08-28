use crate::ext::gui::GuiHandler;
use crate::plugin::Plugin;
use crate::plugin_instance::*;
use clap_sys::host::clap_host_t;
use clap_sys::plugin::clap_plugin_descriptor_t;
use clap_sys::plugin::clap_plugin_t;
use std::ffi::c_void;

pub struct PluginEntry;

impl PluginEntry {
    pub unsafe fn create_plugin<P: Plugin>(
        host: *const clap_host_t,
        descriptor: *const clap_plugin_descriptor_t,
    ) -> *const clap_plugin_t {
        // Factory callbacks are C ABI entry points. A plugin's constructor
        // and metadata declarations are user code, so contain a panic and
        // report creation failure instead of unwinding into the host.
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            Self::create_plugin_unchecked::<P>(host, descriptor)
        }))
        .unwrap_or(std::ptr::null())
    }

    unsafe fn create_plugin_unchecked<P: Plugin>(
        host: *const clap_host_t,
        descriptor: *const clap_plugin_descriptor_t,
    ) -> *const clap_plugin_t {
        let instance = Box::new(PluginInstance::<P>::new(unsafe {
            crate::plugin::HostHandle::from_raw(host)
        }));

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
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            Self::create_plugin_unchecked::<P>(host, descriptor)
        }))
        .unwrap_or(std::ptr::null())
    }

    unsafe fn create_plugin_unchecked<P: Plugin + GuiHandler>(
        host: *const clap_host_t,
        descriptor: *const clap_plugin_descriptor_t,
    ) -> *const clap_plugin_t {
        let instance = Box::new(PluginInstanceWithGui::<P>::new(unsafe {
            crate::plugin::HostHandle::from_raw(host)
        }));

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ParameterInfo;
    use crate::ext::gui::GuiHandler;
    use crate::plugin::AudioProcessor;
    use crate::process::{MAX_PROCESS_FRAMES, ProcessContext};
    use clap_sys::events::{
        CLAP_CORE_EVENT_SPACE_ID, CLAP_EVENT_PARAM_VALUE, clap_event_header_t,
        clap_event_param_value_t, clap_input_events_t,
    };
    use clap_sys::ext::gui::{CLAP_EXT_GUI, clap_plugin_gui_t};
    use clap_sys::ext::params::{CLAP_EXT_PARAMS, clap_plugin_params_t};
    use clap_sys::ext::state::CLAP_EXT_STATE;
    use clap_sys::ext::tail::{CLAP_EXT_TAIL, clap_plugin_tail_t};
    use clap_sys::process::{CLAP_PROCESS_CONTINUE, CLAP_PROCESS_ERROR, clap_process_status};
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::ffi::c_void;
    use std::ptr;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};
    use std::sync::{Barrier, Mutex};

    struct TrackingAllocator;

    static TRACKED_ALLOCATION: AtomicPtr<u8> = AtomicPtr::new(ptr::null_mut());
    static TRACKED_ALLOCATION_FREED: AtomicBool = AtomicBool::new(false);

    unsafe impl GlobalAlloc for TrackingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            if TRACKED_ALLOCATION
                .compare_exchange(ptr, ptr::null_mut(), Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                TRACKED_ALLOCATION_FREED.store(true, Ordering::SeqCst);
            }
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: TrackingAllocator = TrackingAllocator;

    static PLUGIN_DROPS: AtomicUsize = AtomicUsize::new(0);
    // These tests intentionally inspect a process-global drop counter. Keep
    // all TestPlugin lifecycle tests serialized so the counter cannot be
    // changed by a concurrently running test.
    static TEST_PLUGIN_LIFECYCLE_LOCK: Mutex<()> = Mutex::new(());

    struct TestPlugin;

    impl Drop for TestPlugin {
        fn drop(&mut self) {
            PLUGIN_DROPS.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl Plugin for TestPlugin {
        type AudioProcessor = ();

        fn new(_host: crate::plugin::HostHandle) -> Self {
            Self
        }

        fn activate(
            &mut self,
            _sample_rate: f64,
            _min_frames: u32,
            _max_frames: u32,
        ) -> Option<Self::AudioProcessor> {
            Some(())
        }

        fn get_parameter(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_parameter(&mut self, _id: u32, _value: f64) {}
    }

    impl GuiHandler for TestPlugin {}

    fn track(plugin: *const clap_plugin_t) {
        assert!(!plugin.is_null());
        TRACKED_ALLOCATION_FREED.store(false, Ordering::SeqCst);
        TRACKED_ALLOCATION.store(plugin as *mut u8, Ordering::SeqCst);
    }

    fn assert_tracked_allocation_was_freed() {
        assert!(TRACKED_ALLOCATION_FREED.load(Ordering::SeqCst));
        assert!(TRACKED_ALLOCATION.load(Ordering::SeqCst).is_null());
    }

    #[test]
    fn destroy_callbacks_free_plugin_instance_and_outer_allocation() {
        let _lock = TEST_PLUGIN_LIFECYCLE_LOCK.lock().unwrap();
        PLUGIN_DROPS.store(0, Ordering::SeqCst);

        let plugin = unsafe { PluginEntry::create_plugin::<TestPlugin>(ptr::null(), ptr::null()) };
        track(plugin);
        unsafe { ((*plugin).destroy.unwrap())(plugin) };
        assert_tracked_allocation_was_freed();
        assert_eq!(PLUGIN_DROPS.load(Ordering::SeqCst), 1);

        let plugin =
            unsafe { PluginEntryWithGui::create_plugin::<TestPlugin>(ptr::null(), ptr::null()) };
        track(plugin);
        unsafe { ((*plugin).destroy.unwrap())(plugin) };
        assert_tracked_allocation_was_freed();
        assert_eq!(PLUGIN_DROPS.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn activation_rejects_oversized_host_block() {
        let _lock = TEST_PLUGIN_LIFECYCLE_LOCK.lock().unwrap();
        let plugin = unsafe { PluginEntry::create_plugin::<TestPlugin>(ptr::null(), ptr::null()) };
        assert!(unsafe { ((*plugin).init.unwrap())(plugin) });
        assert!(!unsafe {
            ((*plugin).activate.unwrap())(plugin, 48_000.0, 1, MAX_PROCESS_FRAMES.saturating_add(1))
        });
        unsafe { ((*plugin).destroy.unwrap())(plugin) };
    }

    struct ConcurrentState {
        entered: Barrier,
        release: Barrier,
        audio_active: AtomicBool,
        gui_observed_audio: AtomicBool,
        tail_gui_entered: Barrier,
        tail_gui_release: Barrier,
        mutable_gui_active: AtomicBool,
    }

    struct ConcurrentPlugin {
        state: Arc<ConcurrentState>,
    }

    struct ConcurrentProcessor {
        state: Arc<ConcurrentState>,
    }

    impl Plugin for ConcurrentPlugin {
        type AudioProcessor = ConcurrentProcessor;

        fn new(_host: crate::plugin::HostHandle) -> Self {
            Self {
                state: Arc::new(ConcurrentState {
                    entered: Barrier::new(2),
                    release: Barrier::new(2),
                    audio_active: AtomicBool::new(false),
                    gui_observed_audio: AtomicBool::new(false),
                    tail_gui_entered: Barrier::new(2),
                    tail_gui_release: Barrier::new(2),
                    mutable_gui_active: AtomicBool::new(false),
                }),
            }
        }

        fn activate(
            &mut self,
            _sample_rate: f64,
            _min_frames: u32,
            _max_frames: u32,
        ) -> Option<Self::AudioProcessor> {
            Some(ConcurrentProcessor {
                state: self.state.clone(),
            })
        }

        fn get_parameter(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_parameter(&mut self, _id: u32, _value: f64) {}

        fn tail(&self) -> u32 {
            77
        }
    }

    impl AudioProcessor for ConcurrentProcessor {
        fn process(&mut self, _process: ProcessContext) -> clap_process_status {
            self.state.audio_active.store(true, Ordering::SeqCst);
            self.state.entered.wait();
            self.state.release.wait();
            self.state.audio_active.store(false, Ordering::SeqCst);
            CLAP_PROCESS_CONTINUE
        }

        fn set_parameter(&mut self, _id: u32, _value: f64) {}
    }

    impl GuiHandler for ConcurrentPlugin {
        fn gui_get_size(&self) -> Option<(u32, u32)> {
            self.state.entered.wait();
            self.state.gui_observed_audio.store(
                self.state.audio_active.load(Ordering::SeqCst),
                Ordering::SeqCst,
            );
            self.state.release.wait();
            Some((320, 200))
        }

        fn gui_show(&mut self) -> bool {
            self.state.mutable_gui_active.store(true, Ordering::SeqCst);
            self.state.tail_gui_entered.wait();
            self.state.tail_gui_release.wait();
            self.state.mutable_gui_active.store(false, Ordering::SeqCst);
            true
        }
    }

    #[test]
    fn process_can_overlap_get_extension_and_gui_read_callbacks() {
        let plugin = unsafe {
            PluginEntryWithGui::create_plugin::<ConcurrentPlugin>(ptr::null(), ptr::null())
        };
        assert!(unsafe { ((*plugin).init.unwrap())(plugin) });
        assert!(unsafe { ((*plugin).activate.unwrap())(plugin, 48_000.0, 1, 64) });

        let instance =
            unsafe { &*((*plugin).plugin_data as *const PluginInstanceWithGui<ConcurrentPlugin>) };
        let state = unsafe { instance.controller() }.state.clone();
        let process = clap_sys::process::clap_process_t {
            steady_time: 0,
            frames_count: 64,
            transport: ptr::null(),
            audio_inputs: ptr::null(),
            audio_outputs: ptr::null_mut(),
            audio_inputs_count: 0,
            audio_outputs_count: 0,
            in_events: ptr::null(),
            out_events: ptr::null(),
        };
        let plugin_address = plugin as usize;
        let process_address = &process as *const _ as usize;

        std::thread::scope(|scope| {
            scope.spawn(move || {
                let plugin = plugin_address as *const clap_plugin_t;
                let process = process_address as *const clap_sys::process::clap_process_t;
                assert_eq!(
                    unsafe { ((*plugin).process.unwrap())(plugin, process) },
                    CLAP_PROCESS_CONTINUE
                );
            });

            while !state.audio_active.load(Ordering::SeqCst) {
                std::thread::yield_now();
            }
            let tail = unsafe {
                ((*plugin).get_extension.unwrap())(plugin, CLAP_EXT_TAIL.as_ptr().cast())
                    as *const clap_plugin_tail_t
            };
            assert!(!tail.is_null());
            assert_eq!(unsafe { ((*tail).get.unwrap())(plugin) }, 77);

            let gui = unsafe {
                ((*plugin).get_extension.unwrap())(plugin, CLAP_EXT_GUI.as_ptr().cast())
                    as *const clap_plugin_gui_t
            };
            assert!(!gui.is_null());
            let mut width = 0;
            let mut height = 0;
            assert!(unsafe { ((*gui).get_size.unwrap())(plugin, &mut width, &mut height) });
            assert_eq!((width, height), (320, 200));
        });

        assert!(state.gui_observed_audio.load(Ordering::SeqCst));
        unsafe {
            ((*plugin).deactivate.unwrap())(plugin);
            ((*plugin).destroy.unwrap())(plugin);
        }
    }

    #[test]
    fn tail_get_can_overlap_a_mutable_gui_callback() {
        let plugin = unsafe {
            PluginEntryWithGui::create_plugin::<ConcurrentPlugin>(ptr::null(), ptr::null())
        };
        assert!(unsafe { ((*plugin).init.unwrap())(plugin) });
        let instance =
            unsafe { &*((*plugin).plugin_data as *const PluginInstanceWithGui<ConcurrentPlugin>) };
        let state = unsafe { instance.controller() }.state.clone();
        let gui = unsafe {
            ((*plugin).get_extension.unwrap())(plugin, CLAP_EXT_GUI.as_ptr().cast())
                as *const clap_plugin_gui_t
        };
        let tail = unsafe {
            ((*plugin).get_extension.unwrap())(plugin, CLAP_EXT_TAIL.as_ptr().cast())
                as *const clap_plugin_tail_t
        };
        assert!(!gui.is_null());
        assert!(!tail.is_null());

        let plugin_address = plugin as usize;
        let tail_address = tail as usize;
        std::thread::scope(|scope| {
            let state_for_tail = state.clone();
            let tail_thread = scope.spawn(move || {
                state_for_tail.tail_gui_entered.wait();
                let plugin = plugin_address as *const clap_plugin_t;
                let tail = tail_address as *const clap_plugin_tail_t;
                let value = unsafe { ((*tail).get.unwrap())(plugin) };
                let overlapped = state_for_tail.mutable_gui_active.load(Ordering::SeqCst);
                state_for_tail.tail_gui_release.wait();
                (value, overlapped)
            });

            assert!(unsafe { ((*gui).show.unwrap())(plugin) });
            let (value, overlapped) = tail_thread.join().expect("tail thread");
            assert_eq!(value, 77);
            assert!(overlapped);
        });

        unsafe { ((*plugin).destroy.unwrap())(plugin) };
    }

    struct RoutingState {
        controller_sets: AtomicUsize,
        processor_sets: AtomicUsize,
        activations: AtomicUsize,
    }

    struct RoutingPlugin {
        state: Arc<RoutingState>,
    }

    struct RoutingProcessor {
        state: Arc<RoutingState>,
    }

    impl Plugin for RoutingPlugin {
        type AudioProcessor = RoutingProcessor;

        fn new(_host: crate::plugin::HostHandle) -> Self {
            Self {
                state: Arc::new(RoutingState {
                    controller_sets: AtomicUsize::new(0),
                    processor_sets: AtomicUsize::new(0),
                    activations: AtomicUsize::new(0),
                }),
            }
        }

        fn activate(
            &mut self,
            _sample_rate: f64,
            _min_frames: u32,
            _max_frames: u32,
        ) -> Option<Self::AudioProcessor> {
            self.state.activations.fetch_add(1, Ordering::SeqCst);
            Some(RoutingProcessor {
                state: self.state.clone(),
            })
        }

        fn declare_parameters(&self) -> Vec<ParameterInfo> {
            vec![ParameterInfo {
                id: 7,
                name: "Routing".into(),
                module: String::new(),
                min_value: 0.0,
                max_value: 1.0,
                default_value: 0.0,
                is_stepped: false,
            }]
        }

        fn get_parameter(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_parameter(&mut self, _id: u32, _value: f64) {
            self.state.controller_sets.fetch_add(1, Ordering::SeqCst);
        }

        fn tail(&self) -> u32 {
            self.state.controller_sets.load(Ordering::SeqCst) as u32
        }
    }

    impl AudioProcessor for RoutingProcessor {
        fn process(&mut self, _process: ProcessContext) -> clap_process_status {
            CLAP_PROCESS_CONTINUE
        }

        fn set_parameter(&mut self, _id: u32, _value: f64) {
            self.state.processor_sets.fetch_add(1, Ordering::SeqCst);
        }
    }

    unsafe extern "C" fn one_event_size(_list: *const clap_input_events_t) -> u32 {
        1
    }

    unsafe extern "C" fn one_event_get(
        list: *const clap_input_events_t,
        index: u32,
    ) -> *const clap_event_header_t {
        if index == 0 {
            unsafe { (*list).ctx.cast() }
        } else {
            ptr::null()
        }
    }

    fn flush_on_audio_thread(
        plugin: *const clap_plugin_t,
        params: *const clap_plugin_params_t,
        input: &clap_input_events_t,
    ) {
        let plugin_address = plugin as usize;
        let params_address = params as usize;
        let input_address = input as *const clap_input_events_t as usize;
        std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    let plugin = plugin_address as *const clap_plugin_t;
                    let params = params_address as *const clap_plugin_params_t;
                    let input = input_address as *const clap_input_events_t;
                    unsafe { ((*params).flush.unwrap())(plugin, input, ptr::null()) };
                })
                .join()
                .expect("audio-thread flush");
        });
    }

    #[test]
    fn params_flush_routes_by_activation_and_reactivation_is_supported() {
        let plugin =
            unsafe { PluginEntry::create_plugin::<RoutingPlugin>(ptr::null(), ptr::null()) };
        assert!(unsafe { ((*plugin).init.unwrap())(plugin) });
        let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstance<RoutingPlugin>) };
        let state = unsafe { instance.controller() }.state.clone();
        let params = unsafe {
            ((*plugin).get_extension.unwrap())(plugin, CLAP_EXT_PARAMS.as_ptr().cast())
                as *const clap_plugin_params_t
        };
        let tail = unsafe {
            ((*plugin).get_extension.unwrap())(plugin, CLAP_EXT_TAIL.as_ptr().cast())
                as *const clap_plugin_tail_t
        };
        assert!(!params.is_null());
        assert!(!tail.is_null());

        let event = clap_event_param_value_t {
            header: clap_event_header_t {
                size: std::mem::size_of::<clap_event_param_value_t>() as u32,
                time: 0,
                space_id: CLAP_CORE_EVENT_SPACE_ID,
                type_: CLAP_EVENT_PARAM_VALUE,
                flags: 0,
            },
            param_id: 7,
            cookie: ptr::null_mut(),
            note_id: -1,
            port_index: -1,
            channel: -1,
            key: -1,
            value: 0.5,
        };
        let input = clap_input_events_t {
            ctx: (&event as *const clap_event_param_value_t)
                .cast_mut()
                .cast::<c_void>(),
            size: Some(one_event_size),
            get: Some(one_event_get),
        };
        let flush = unsafe { (*params).flush.unwrap() };

        unsafe { flush(plugin, &input, ptr::null()) };
        assert_eq!(state.controller_sets.load(Ordering::SeqCst), 1);
        assert_eq!(unsafe { ((*tail).get.unwrap())(plugin) }, 1);

        assert!(unsafe { ((*plugin).activate.unwrap())(plugin, 48_000.0, 1, 64) });
        flush_on_audio_thread(plugin, params, &input);
        assert_eq!(state.processor_sets.load(Ordering::SeqCst), 1);
        assert_eq!(state.controller_sets.load(Ordering::SeqCst), 1);
        unsafe { ((*plugin).deactivate.unwrap())(plugin) };

        unsafe { flush(plugin, &input, ptr::null()) };
        assert_eq!(state.controller_sets.load(Ordering::SeqCst), 2);
        assert_eq!(unsafe { ((*tail).get.unwrap())(plugin) }, 2);

        assert!(unsafe { ((*plugin).activate.unwrap())(plugin, 96_000.0, 1, 128) });
        flush_on_audio_thread(plugin, params, &input);
        assert_eq!(state.processor_sets.load(Ordering::SeqCst), 2);
        unsafe { ((*plugin).deactivate.unwrap())(plugin) };

        assert_eq!(state.activations.load(Ordering::SeqCst), 2);
        unsafe { ((*plugin).destroy.unwrap())(plugin) };
    }

    struct PanickingConstructorPlugin;

    impl Plugin for PanickingConstructorPlugin {
        type AudioProcessor = ();

        fn new(_host: crate::plugin::HostHandle) -> Self {
            panic!("intentional constructor panic");
        }

        fn activate(
            &mut self,
            _sample_rate: f64,
            _min_frames: u32,
            _max_frames: u32,
        ) -> Option<Self::AudioProcessor> {
            Some(())
        }

        fn get_parameter(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_parameter(&mut self, _id: u32, _value: f64) {}
    }

    impl GuiHandler for PanickingConstructorPlugin {}

    #[test]
    fn factory_contains_constructor_panics() {
        assert!(
            unsafe {
                PluginEntry::create_plugin::<PanickingConstructorPlugin>(ptr::null(), ptr::null())
            }
            .is_null()
        );
        assert!(
            unsafe {
                PluginEntryWithGui::create_plugin::<PanickingConstructorPlugin>(
                    ptr::null(),
                    ptr::null(),
                )
            }
            .is_null()
        );
    }

    struct PanickingInitPlugin;

    impl Plugin for PanickingInitPlugin {
        type AudioProcessor = ();

        fn new(_host: crate::plugin::HostHandle) -> Self {
            Self
        }

        fn init(&mut self) -> bool {
            panic!("intentional init panic");
        }

        fn activate(
            &mut self,
            _sample_rate: f64,
            _min_frames: u32,
            _max_frames: u32,
        ) -> Option<Self::AudioProcessor> {
            Some(())
        }

        fn get_parameter(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_parameter(&mut self, _id: u32, _value: f64) {}
    }

    impl GuiHandler for PanickingInitPlugin {}

    struct PanickingProcessor;

    static PANICKING_PROCESS_CALLS: AtomicUsize = AtomicUsize::new(0);

    impl AudioProcessor for PanickingProcessor {
        fn process(&mut self, _process: ProcessContext) -> clap_process_status {
            PANICKING_PROCESS_CALLS.fetch_add(1, Ordering::SeqCst);
            panic!("intentional process panic");
        }

        fn set_parameter(&mut self, _id: u32, _value: f64) {}
    }

    struct PanickingProcessPlugin;

    impl Plugin for PanickingProcessPlugin {
        type AudioProcessor = PanickingProcessor;

        fn new(_host: crate::plugin::HostHandle) -> Self {
            Self
        }

        fn activate(
            &mut self,
            _sample_rate: f64,
            _min_frames: u32,
            _max_frames: u32,
        ) -> Option<Self::AudioProcessor> {
            Some(PanickingProcessor)
        }

        fn get_parameter(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_parameter(&mut self, _id: u32, _value: f64) {}
    }

    #[test]
    fn raw_lifecycle_and_process_callbacks_contain_panics() {
        let plugin =
            unsafe { PluginEntry::create_plugin::<PanickingInitPlugin>(ptr::null(), ptr::null()) };
        assert!(!unsafe { ((*plugin).init.unwrap())(plugin) });
        unsafe { ((*plugin).destroy.unwrap())(plugin) };

        let plugin = unsafe {
            PluginEntry::create_plugin::<PanickingProcessPlugin>(ptr::null(), ptr::null())
        };
        PANICKING_PROCESS_CALLS.store(0, Ordering::SeqCst);
        assert!(unsafe { ((*plugin).init.unwrap())(plugin) });
        assert!(unsafe { ((*plugin).activate.unwrap())(plugin, 48_000.0, 0, 1) });
        let process = clap_sys::process::clap_process_t {
            steady_time: 0,
            frames_count: 0,
            transport: ptr::null(),
            audio_inputs: ptr::null(),
            audio_outputs: ptr::null_mut(),
            audio_inputs_count: 0,
            audio_outputs_count: 0,
            in_events: ptr::null(),
            out_events: ptr::null(),
        };
        assert_eq!(
            unsafe { ((*plugin).process.unwrap())(plugin, &process) },
            CLAP_PROCESS_ERROR
        );
        assert_eq!(
            unsafe { ((*plugin).process.unwrap())(plugin, &process) },
            CLAP_PROCESS_ERROR
        );
        assert_eq!(PANICKING_PROCESS_CALLS.load(Ordering::SeqCst), 1);
        unsafe { ((*plugin).deactivate.unwrap())(plugin) };
        unsafe { ((*plugin).destroy.unwrap())(plugin) };
    }

    const PANIC_NONE: usize = 0;
    const PANIC_START: usize = 1;
    const PANIC_STOP: usize = 2;
    const PANIC_RESET: usize = 3;
    const PANIC_PROCESS: usize = 4;
    const PANIC_DEACTIVATE: usize = 5;

    static LIFECYCLE_PANIC_LOCK: Mutex<()> = Mutex::new(());
    static LIFECYCLE_PANIC_STAGE: AtomicUsize = AtomicUsize::new(PANIC_NONE);
    static LIFECYCLE_PANIC_CALLBACKS: AtomicUsize = AtomicUsize::new(0);
    static LIFECYCLE_PROCESS_CALLS: AtomicUsize = AtomicUsize::new(0);
    static LIFECYCLE_ACTIVATIONS: AtomicUsize = AtomicUsize::new(0);
    static LIFECYCLE_DEACTIVATIONS: AtomicUsize = AtomicUsize::new(0);
    static LIFECYCLE_PROCESSOR_DROPS: AtomicUsize = AtomicUsize::new(0);
    static LIFECYCLE_PLUGIN_DROPS: AtomicUsize = AtomicUsize::new(0);

    fn panic_at(stage: usize) {
        if LIFECYCLE_PANIC_STAGE.load(Ordering::SeqCst) == stage {
            LIFECYCLE_PANIC_CALLBACKS.fetch_add(1, Ordering::SeqCst);
            panic!("intentional lifecycle panic at stage {stage}");
        }
    }

    fn reset_lifecycle_counters(stage: usize) {
        LIFECYCLE_PANIC_STAGE.store(stage, Ordering::SeqCst);
        LIFECYCLE_PANIC_CALLBACKS.store(0, Ordering::SeqCst);
        LIFECYCLE_PROCESS_CALLS.store(0, Ordering::SeqCst);
        LIFECYCLE_ACTIVATIONS.store(0, Ordering::SeqCst);
        LIFECYCLE_DEACTIVATIONS.store(0, Ordering::SeqCst);
        LIFECYCLE_PROCESSOR_DROPS.store(0, Ordering::SeqCst);
        LIFECYCLE_PLUGIN_DROPS.store(0, Ordering::SeqCst);
    }

    fn empty_process() -> clap_sys::process::clap_process_t {
        clap_sys::process::clap_process_t {
            steady_time: 0,
            frames_count: 0,
            transport: ptr::null(),
            audio_inputs: ptr::null(),
            audio_outputs: ptr::null_mut(),
            audio_inputs_count: 0,
            audio_outputs_count: 0,
            in_events: ptr::null(),
            out_events: ptr::null(),
        }
    }

    struct LifecyclePanicProcessor;

    impl Drop for LifecyclePanicProcessor {
        fn drop(&mut self) {
            LIFECYCLE_PROCESSOR_DROPS.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl AudioProcessor for LifecyclePanicProcessor {
        fn start_processing(&mut self) -> bool {
            panic_at(PANIC_START);
            true
        }

        fn stop_processing(&mut self) {
            panic_at(PANIC_STOP);
        }

        fn reset(&mut self) {
            panic_at(PANIC_RESET);
        }

        fn process(&mut self, _process: ProcessContext) -> clap_process_status {
            LIFECYCLE_PROCESS_CALLS.fetch_add(1, Ordering::SeqCst);
            panic_at(PANIC_PROCESS);
            CLAP_PROCESS_CONTINUE
        }

        fn set_parameter(&mut self, _id: u32, _value: f64) {}
    }

    struct LifecyclePanicPlugin;

    impl Drop for LifecyclePanicPlugin {
        fn drop(&mut self) {
            LIFECYCLE_PLUGIN_DROPS.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl Plugin for LifecyclePanicPlugin {
        type AudioProcessor = LifecyclePanicProcessor;

        fn new(_host: crate::plugin::HostHandle) -> Self {
            Self
        }

        fn activate(
            &mut self,
            _sample_rate: f64,
            _min_frames: u32,
            _max_frames: u32,
        ) -> Option<Self::AudioProcessor> {
            LIFECYCLE_ACTIVATIONS.fetch_add(1, Ordering::SeqCst);
            Some(LifecyclePanicProcessor)
        }

        fn deactivate(&mut self, _processor: Self::AudioProcessor) {
            LIFECYCLE_DEACTIVATIONS.fetch_add(1, Ordering::SeqCst);
            panic_at(PANIC_DEACTIVATE);
        }

        fn get_parameter(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_parameter(&mut self, _id: u32, _value: f64) {}
    }

    #[test]
    fn audio_callback_panics_poison_until_successful_deactivation() {
        let _lock = LIFECYCLE_PANIC_LOCK.lock().unwrap();
        let process = empty_process();

        for stage in [PANIC_START, PANIC_STOP, PANIC_RESET, PANIC_PROCESS] {
            reset_lifecycle_counters(stage);
            let plugin = unsafe {
                PluginEntry::create_plugin::<LifecyclePanicPlugin>(ptr::null(), ptr::null())
            };
            assert!(unsafe { ((*plugin).init.unwrap())(plugin) });
            assert!(unsafe { ((*plugin).activate.unwrap())(plugin, 48_000.0, 0, 1) });

            match stage {
                PANIC_START => {
                    assert!(!unsafe { ((*plugin).start_processing.unwrap())(plugin) });
                    assert!(!unsafe { ((*plugin).start_processing.unwrap())(plugin) });
                }
                PANIC_STOP => unsafe {
                    assert!(((*plugin).start_processing.unwrap())(plugin));
                    ((*plugin).stop_processing.unwrap())(plugin);
                    ((*plugin).stop_processing.unwrap())(plugin);
                },
                PANIC_RESET => unsafe {
                    ((*plugin).reset.unwrap())(plugin);
                    ((*plugin).reset.unwrap())(plugin);
                },
                PANIC_PROCESS => {
                    assert_eq!(
                        unsafe { ((*plugin).process.unwrap())(plugin, &process) },
                        CLAP_PROCESS_ERROR
                    );
                }
                _ => unreachable!(),
            }

            assert_eq!(LIFECYCLE_PANIC_CALLBACKS.load(Ordering::SeqCst), 1);
            assert_eq!(
                unsafe { ((*plugin).process.unwrap())(plugin, &process) },
                CLAP_PROCESS_ERROR
            );
            assert_eq!(
                LIFECYCLE_PROCESS_CALLS.load(Ordering::SeqCst),
                usize::from(stage == PANIC_PROCESS)
            );
            assert!(!unsafe { ((*plugin).activate.unwrap())(plugin, 48_000.0, 0, 1) });

            LIFECYCLE_PANIC_STAGE.store(PANIC_NONE, Ordering::SeqCst);
            unsafe { ((*plugin).deactivate.unwrap())(plugin) };
            assert_eq!(LIFECYCLE_DEACTIVATIONS.load(Ordering::SeqCst), 1);
            assert!(unsafe { ((*plugin).activate.unwrap())(plugin, 48_000.0, 0, 1) });
            unsafe { ((*plugin).deactivate.unwrap())(plugin) };
            unsafe { ((*plugin).destroy.unwrap())(plugin) };

            assert_eq!(LIFECYCLE_ACTIVATIONS.load(Ordering::SeqCst), 2);
            assert_eq!(LIFECYCLE_DEACTIVATIONS.load(Ordering::SeqCst), 2);
            assert_eq!(LIFECYCLE_PROCESSOR_DROPS.load(Ordering::SeqCst), 2);
            assert_eq!(LIFECYCLE_PLUGIN_DROPS.load(Ordering::SeqCst), 1);
        }
    }

    #[test]
    fn deactivation_panic_keeps_instance_poisoned_until_destroy() {
        let _lock = LIFECYCLE_PANIC_LOCK.lock().unwrap();
        reset_lifecycle_counters(PANIC_DEACTIVATE);
        let process = empty_process();
        let plugin =
            unsafe { PluginEntry::create_plugin::<LifecyclePanicPlugin>(ptr::null(), ptr::null()) };
        assert!(unsafe { ((*plugin).init.unwrap())(plugin) });
        assert!(unsafe { ((*plugin).activate.unwrap())(plugin, 48_000.0, 0, 1) });

        unsafe { ((*plugin).deactivate.unwrap())(plugin) };
        assert_eq!(LIFECYCLE_PANIC_CALLBACKS.load(Ordering::SeqCst), 1);
        assert_eq!(LIFECYCLE_DEACTIVATIONS.load(Ordering::SeqCst), 1);
        assert_eq!(LIFECYCLE_PROCESSOR_DROPS.load(Ordering::SeqCst), 1);
        assert_eq!(
            unsafe { ((*plugin).process.unwrap())(plugin, &process) },
            CLAP_PROCESS_ERROR
        );
        assert!(!unsafe { ((*plugin).activate.unwrap())(plugin, 48_000.0, 0, 1) });

        // A duplicate host callback has no processor to hand back and must not
        // silently clear poison left by the failed deactivation.
        LIFECYCLE_PANIC_STAGE.store(PANIC_NONE, Ordering::SeqCst);
        unsafe { ((*plugin).deactivate.unwrap())(plugin) };
        assert_eq!(LIFECYCLE_DEACTIVATIONS.load(Ordering::SeqCst), 1);
        assert!(!unsafe { ((*plugin).activate.unwrap())(plugin, 48_000.0, 0, 1) });

        unsafe { ((*plugin).destroy.unwrap())(plugin) };
        assert_eq!(LIFECYCLE_ACTIVATIONS.load(Ordering::SeqCst), 1);
        assert_eq!(LIFECYCLE_PROCESSOR_DROPS.load(Ordering::SeqCst), 1);
        assert_eq!(LIFECYCLE_PLUGIN_DROPS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn init_failure_rolls_back_extension_tables_for_plain_and_gui_plugins() {
        let plugin =
            unsafe { PluginEntry::create_plugin::<PanickingInitPlugin>(ptr::null(), ptr::null()) };
        assert!(!unsafe { ((*plugin).init.unwrap())(plugin) });
        assert!(
            unsafe { ((*plugin).get_extension.unwrap())(plugin, CLAP_EXT_STATE.as_ptr().cast()) }
                .is_null()
        );
        assert!(
            unsafe { ((*plugin).get_extension.unwrap())(plugin, CLAP_EXT_TAIL.as_ptr().cast()) }
                .is_null()
        );
        unsafe { ((*plugin).destroy.unwrap())(plugin) };

        let plugin = unsafe {
            PluginEntryWithGui::create_plugin::<PanickingInitPlugin>(ptr::null(), ptr::null())
        };
        assert!(!unsafe { ((*plugin).init.unwrap())(plugin) });
        assert!(
            unsafe { ((*plugin).get_extension.unwrap())(plugin, CLAP_EXT_STATE.as_ptr().cast()) }
                .is_null()
        );
        assert!(
            unsafe { ((*plugin).get_extension.unwrap())(plugin, CLAP_EXT_TAIL.as_ptr().cast()) }
                .is_null()
        );
        assert!(
            unsafe { ((*plugin).get_extension.unwrap())(plugin, CLAP_EXT_GUI.as_ptr().cast()) }
                .is_null()
        );
        unsafe { ((*plugin).destroy.unwrap())(plugin) };
    }

    static POST_ACTIVATION_TAIL_CALLS: AtomicUsize = AtomicUsize::new(0);
    static POST_ACTIVATION_DEACTIVATIONS: AtomicUsize = AtomicUsize::new(0);

    struct TailPanicProcessor;

    impl AudioProcessor for TailPanicProcessor {
        fn process(&mut self, _process: ProcessContext) -> clap_process_status {
            CLAP_PROCESS_CONTINUE
        }

        fn set_parameter(&mut self, _id: u32, _value: f64) {}
    }

    struct PanickingPostActivationTailPlugin;

    impl Plugin for PanickingPostActivationTailPlugin {
        type AudioProcessor = TailPanicProcessor;

        fn new(_host: crate::plugin::HostHandle) -> Self {
            Self
        }

        fn activate(
            &mut self,
            _sample_rate: f64,
            _min_frames: u32,
            _max_frames: u32,
        ) -> Option<Self::AudioProcessor> {
            Some(TailPanicProcessor)
        }

        fn deactivate(&mut self, _processor: Self::AudioProcessor) {
            POST_ACTIVATION_DEACTIVATIONS.fetch_add(1, Ordering::SeqCst);
        }

        fn get_parameter(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_parameter(&mut self, _id: u32, _value: f64) {}

        fn tail(&self) -> u32 {
            let call = POST_ACTIVATION_TAIL_CALLS.fetch_add(1, Ordering::SeqCst) + 1;
            if call == 3 {
                panic!("intentional post-activation tail panic");
            }
            0
        }
    }

    #[test]
    fn activation_tail_panic_rolls_back_processor_and_allows_retry() {
        POST_ACTIVATION_TAIL_CALLS.store(0, Ordering::SeqCst);
        POST_ACTIVATION_DEACTIVATIONS.store(0, Ordering::SeqCst);

        let plugin = unsafe {
            PluginEntry::create_plugin::<PanickingPostActivationTailPlugin>(
                ptr::null(),
                ptr::null(),
            )
        };
        assert!(unsafe { ((*plugin).init.unwrap())(plugin) });
        assert!(!unsafe { ((*plugin).activate.unwrap())(plugin, 48_000.0, 1, 64) });
        assert_eq!(POST_ACTIVATION_DEACTIVATIONS.load(Ordering::SeqCst), 1);

        assert!(unsafe { ((*plugin).activate.unwrap())(plugin, 48_000.0, 1, 64) });
        unsafe { ((*plugin).deactivate.unwrap())(plugin) };
        assert_eq!(POST_ACTIVATION_DEACTIVATIONS.load(Ordering::SeqCst), 2);
        unsafe { ((*plugin).destroy.unwrap())(plugin) };
    }

    // ======= Audio ports activation =======

    use crate::ext::audio_ports::AudioPortInfo;
    use clap_sys::ext::audio_ports_activation::{
        CLAP_EXT_AUDIO_PORTS_ACTIVATION, CLAP_EXT_AUDIO_PORTS_ACTIVATION_COMPAT,
        clap_plugin_audio_ports_activation_t,
    };

    static ACTIVATION_CALLS: Mutex<Vec<(bool, u32, bool, u32)>> = Mutex::new(Vec::new());

    struct ActivationPlugin;

    impl Plugin for ActivationPlugin {
        type AudioProcessor = ();

        fn new(_host: crate::plugin::HostHandle) -> Self {
            Self
        }

        fn audio_ports_config(&self) -> Vec<AudioPortInfo> {
            vec![
                AudioPortInfo {
                    id: 0,
                    name: "Main In".to_string(),
                    channel_count: 2,
                    is_main: true,
                    is_input: true,
                },
                AudioPortInfo {
                    id: 1,
                    name: "Sidechain".to_string(),
                    channel_count: 2,
                    is_main: false,
                    is_input: true,
                },
                AudioPortInfo {
                    id: 2,
                    name: "Main Out".to_string(),
                    channel_count: 2,
                    is_main: true,
                    is_input: false,
                },
            ]
        }

        fn set_audio_port_active(
            &mut self,
            is_input: bool,
            port_index: u32,
            is_active: bool,
            sample_size: u32,
        ) -> bool {
            ACTIVATION_CALLS
                .lock()
                .unwrap()
                .push((is_input, port_index, is_active, sample_size));
            true
        }

        fn activate(
            &mut self,
            _sample_rate: f64,
            _min_frames: u32,
            _max_frames: u32,
        ) -> Option<Self::AudioProcessor> {
            Some(())
        }

        fn get_parameter(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_parameter(&mut self, _id: u32, _value: f64) {}
    }

    #[test]
    fn audio_ports_activation_validates_the_index_and_forwards_to_the_plugin() {
        ACTIVATION_CALLS.lock().unwrap().clear();

        let plugin =
            unsafe { PluginEntry::create_plugin::<ActivationPlugin>(ptr::null(), ptr::null()) };
        assert!(unsafe { ((*plugin).init.unwrap())(plugin) });

        let ext = unsafe {
            ((*plugin).get_extension.unwrap())(
                plugin,
                CLAP_EXT_AUDIO_PORTS_ACTIVATION.as_ptr().cast(),
            )
        } as *const clap_plugin_audio_ports_activation_t;
        assert!(
            !ext.is_null(),
            "plugins with audio ports must expose clap.audio-ports-activation/2"
        );

        // Hosts may still probe the draft id; both must resolve.
        let compat = unsafe {
            ((*plugin).get_extension.unwrap())(
                plugin,
                CLAP_EXT_AUDIO_PORTS_ACTIVATION_COMPAT.as_ptr().cast(),
            )
        };
        assert_eq!(compat, ext as *const c_void);

        let set_active = unsafe { (*ext).set_active.unwrap() };
        // Both declared input ports and the output port are addressable.
        assert!(unsafe { set_active(plugin, true, 1, false, 32) });
        assert!(unsafe { set_active(plugin, false, 0, true, 0) });
        // Out-of-range indices are rejected before the plugin sees them.
        assert!(!unsafe { set_active(plugin, true, 2, false, 32) });
        assert!(!unsafe { set_active(plugin, false, 1, false, 32) });

        assert_eq!(
            ACTIVATION_CALLS.lock().unwrap().as_slice(),
            &[(true, 1, false, 32), (false, 0, true, 0)],
            "only in-range requests reach the plugin callback"
        );

        let can_activate = unsafe { (*ext).can_activate_while_processing.unwrap() };
        assert!(
            !unsafe { can_activate(plugin) },
            "activation while processing is off by default"
        );

        unsafe { ((*plugin).destroy.unwrap())(plugin) };
    }

    #[test]
    fn plugins_without_audio_ports_do_not_expose_the_activation_extension() {
        let _lock = TEST_PLUGIN_LIFECYCLE_LOCK.lock().unwrap();
        let plugin = unsafe { PluginEntry::create_plugin::<TestPlugin>(ptr::null(), ptr::null()) };
        assert!(unsafe { ((*plugin).init.unwrap())(plugin) });
        let ext = unsafe {
            ((*plugin).get_extension.unwrap())(
                plugin,
                CLAP_EXT_AUDIO_PORTS_ACTIVATION.as_ptr().cast(),
            )
        };
        assert!(ext.is_null());
        unsafe { ((*plugin).destroy.unwrap())(plugin) };
    }

    /// Port declarations the next `PropActivationPlugin` will report, as
    /// (inputs, outputs). Read once per instance at `init`.
    static PROP_PORT_COUNTS: Mutex<(u32, u32)> = Mutex::new((0, 0));
    /// Indices the plugin callback actually saw, per direction.
    static PROP_FORWARDED: Mutex<Vec<(bool, u32)>> = Mutex::new(Vec::new());
    /// `PROP_PORT_COUNTS`/`PROP_FORWARDED` are process-global.
    static PROP_ACTIVATION_LOCK: Mutex<()> = Mutex::new(());

    struct PropActivationPlugin;

    impl Plugin for PropActivationPlugin {
        type AudioProcessor = ();

        fn new(_host: crate::plugin::HostHandle) -> Self {
            Self
        }

        fn audio_ports_config(&self) -> Vec<AudioPortInfo> {
            let (inputs, outputs) = *PROP_PORT_COUNTS.lock().unwrap();
            let mut ports = Vec::new();
            for direction_is_input in [true, false] {
                let count = if direction_is_input { inputs } else { outputs };
                for index in 0..count {
                    ports.push(AudioPortInfo {
                        id: index,
                        name: format!("Port {index}"),
                        channel_count: 2,
                        is_main: index == 0,
                        is_input: direction_is_input,
                    });
                }
            }
            ports
        }

        fn set_audio_port_active(
            &mut self,
            is_input: bool,
            port_index: u32,
            _is_active: bool,
            _sample_size: u32,
        ) -> bool {
            PROP_FORWARDED.lock().unwrap().push((is_input, port_index));
            true
        }

        fn activate(
            &mut self,
            _sample_rate: f64,
            _min_frames: u32,
            _max_frames: u32,
        ) -> Option<Self::AudioProcessor> {
            Some(())
        }

        fn get_parameter(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_parameter(&mut self, _id: u32, _value: f64) {}
    }

    proptest::proptest! {
        /// For any declared port topology and any index a host may pass —
        /// including wildly out-of-range ones — the guard must forward exactly
        /// the in-range requests and reject the rest. An out-of-range index
        /// must never reach the plugin, and an in-range one must never be
        /// silently dropped.
        #[test]
        fn port_activation_forwards_exactly_the_declared_indices(
            inputs in 0u32..4,
            outputs in 0u32..4,
            probe_is_input in proptest::prelude::any::<bool>(),
            probe_index in 0u32..8,
            probe_active in proptest::prelude::any::<bool>(),
            sample_size in proptest::sample::select(vec![0u32, 32u32, 64u32]),
        ) {
            let _serialize = PROP_ACTIVATION_LOCK.lock().unwrap();
            *PROP_PORT_COUNTS.lock().unwrap() = (inputs, outputs);
            PROP_FORWARDED.lock().unwrap().clear();

            let plugin = unsafe {
                PluginEntry::create_plugin::<PropActivationPlugin>(ptr::null(), ptr::null())
            };
            let initialized = unsafe { ((*plugin).init.unwrap())(plugin) };
            proptest::prop_assert!(initialized, "init must succeed");

            let ext = unsafe {
                ((*plugin).get_extension.unwrap())(
                    plugin,
                    CLAP_EXT_AUDIO_PORTS_ACTIVATION.as_ptr().cast(),
                )
            } as *const clap_plugin_audio_ports_activation_t;

            let declared = if probe_is_input { inputs } else { outputs };
            let in_range = probe_index < declared;

            if ext.is_null() {
                // Only a plugin with no ports at all may hide the extension,
                // and then no index can be in range.
                proptest::prop_assert_eq!(inputs + outputs, 0);
                proptest::prop_assert!(!in_range);
            } else {
                let set_active = unsafe { (*ext).set_active.unwrap() };
                let accepted = unsafe {
                    set_active(plugin, probe_is_input, probe_index, probe_active, sample_size)
                };
                proptest::prop_assert_eq!(
                    accepted,
                    in_range,
                    "index {} with {} declared port(s) in that direction",
                    probe_index,
                    declared
                );
                let forwarded = PROP_FORWARDED.lock().unwrap().clone();
                if in_range {
                    proptest::prop_assert_eq!(
                        forwarded.as_slice(),
                        &[(probe_is_input, probe_index)]
                    );
                } else {
                    proptest::prop_assert!(
                        forwarded.is_empty(),
                        "an out-of-range index must not reach the plugin"
                    );
                }
            }

            unsafe { ((*plugin).destroy.unwrap())(plugin) };
        }
    }
}
