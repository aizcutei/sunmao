#![allow(dead_code)]

use crate::vst3_test_api::*;
use std::ffi::c_void;

pub struct ActiveProcessor {
    component: *mut c_void,
    audio: *mut c_void,
}

impl ActiveProcessor {
    pub unsafe fn new(factory: *mut c_void, max_frames: i32) -> Self {
        assert!(!factory.is_null());
        let factory_vtbl = unsafe { *(factory as *const *const IPluginFactoryVtbl) };
        assert!(!factory_vtbl.is_null());
        let mut class_info = unsafe { std::mem::zeroed::<PClassInfoData>() };
        assert_eq!(
            unsafe { ((*factory_vtbl).get_class_info)(factory, 0, &mut class_info) },
            kResultOk
        );

        let mut component = std::ptr::null_mut();
        assert_eq!(
            unsafe {
                ((*factory_vtbl).create_instance)(
                    factory,
                    class_info.cid.as_ptr().cast::<char8>(),
                    vst_iid::IComponent.as_ptr().cast::<char8>(),
                    &mut component,
                )
            },
            kResultOk
        );
        assert!(!component.is_null());
        unsafe { ((*factory_vtbl).unknown.release)(factory) };

        let component_vtbl = unsafe { *(component as *const *const IComponentVtbl) };
        assert_eq!(
            unsafe { ((*component_vtbl).base.initialize)(component, std::ptr::null_mut()) },
            kResultOk
        );

        let mut audio = std::ptr::null_mut();
        assert_eq!(
            unsafe {
                ((*component_vtbl).base.unknown.query_interface)(
                    component,
                    &vst_iid::IAudioProcessor,
                    &mut audio,
                )
            },
            kResultOk
        );
        assert!(!audio.is_null());
        let audio_vtbl = unsafe { *(audio as *const *const IAudioProcessorVtbl) };
        let mut setup = ProcessSetup {
            process_mode: 0,
            symbolic_sample_size: SymbolicSampleSizes::kSample32,
            max_samples_per_block: max_frames,
            sample_rate: 48_000.0,
        };
        assert_eq!(
            unsafe { ((*audio_vtbl).setup_processing)(audio, &mut setup) },
            kResultOk
        );
        assert_eq!(
            unsafe { ((*component_vtbl).set_active)(component, 1) },
            kResultOk
        );
        assert_eq!(
            unsafe { ((*audio_vtbl).set_processing)(audio, 1) },
            kResultOk
        );

        Self { component, audio }
    }

    pub unsafe fn process(&mut self, data: &mut ProcessData) -> tresult {
        let audio_vtbl = unsafe { *(self.audio as *const *const IAudioProcessorVtbl) };
        unsafe { ((*audio_vtbl).process)(self.audio, data) }
    }
}

impl Drop for ActiveProcessor {
    fn drop(&mut self) {
        unsafe {
            let audio_vtbl = *(self.audio as *const *const IAudioProcessorVtbl);
            let component_vtbl = *(self.component as *const *const IComponentVtbl);
            ((*audio_vtbl).set_processing)(self.audio, 0);
            ((*component_vtbl).set_active)(self.component, 0);
            ((*component_vtbl).base.terminate)(self.component);
            ((*audio_vtbl).unknown.release)(self.audio);
            ((*component_vtbl).base.unknown.release)(self.component);
        }
    }
}

#[repr(C)]
pub struct TestEventList {
    vtbl: *const IEventListVtbl,
    event: Event,
}

impl TestEventList {
    pub fn note_on(sample_offset: i32, pitch: i16) -> Self {
        Self {
            vtbl: &EVENT_LIST_VTBL,
            event: Event {
                bus_index: 0,
                sample_offset,
                ppq_position: 0.0,
                flags: 0,
                type_: EventTypes::kNoteOnEvent,
                event: EventData {
                    note_on: NoteOnEvent {
                        channel: 0,
                        pitch,
                        tuning: 0.0,
                        velocity: 1.0,
                        length: 0,
                        note_id: 1,
                    },
                },
            },
        }
    }

    pub fn as_raw(&mut self) -> *mut c_void {
        self as *mut _ as *mut c_void
    }
}

unsafe extern "system" fn event_query_interface(
    _this: *mut c_void,
    _iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    unsafe { *obj = std::ptr::null_mut() };
    kNoInterface
}

unsafe extern "system" fn event_add_ref(_this: *mut c_void) -> uint32 {
    1
}

unsafe extern "system" fn event_release(_this: *mut c_void) -> uint32 {
    1
}

unsafe extern "system" fn event_count(_this: *mut c_void) -> int32 {
    1
}

unsafe extern "system" fn event_get(this: *mut c_void, index: int32, event: *mut Event) -> tresult {
    if index != 0 || event.is_null() {
        return kInvalidArgument;
    }
    unsafe { *event = (*(this as *const TestEventList)).event };
    kResultOk
}

unsafe extern "system" fn event_add(_this: *mut c_void, _event: *mut Event) -> tresult {
    kNotImplemented
}

static EVENT_LIST_VTBL: IEventListVtbl = IEventListVtbl {
    unknown: IUnknownVtbl {
        query_interface: event_query_interface,
        add_ref: event_add_ref,
        release: event_release,
    },
    get_event_count: event_count,
    get_event: event_get,
    add_event: event_add,
};
