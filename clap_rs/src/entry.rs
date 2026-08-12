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
    use crate::process::ProcessContext;
    use clap_sys::events::{
        CLAP_CORE_EVENT_SPACE_ID, CLAP_EVENT_PARAM_VALUE, clap_event_header_t,
        clap_event_param_value_t, clap_input_events_t,
    };
    use clap_sys::ext::gui::{CLAP_EXT_GUI, clap_plugin_gui_t};
    use clap_sys::ext::params::{CLAP_EXT_PARAMS, clap_plugin_params_t};
    use clap_sys::ext::tail::{CLAP_EXT_TAIL, clap_plugin_tail_t};
    use clap_sys::process::{CLAP_PROCESS_CONTINUE, clap_process_status};
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::ffi::c_void;
    use std::ptr;
    use std::sync::Arc;
    use std::sync::Barrier;
    use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};

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
}
