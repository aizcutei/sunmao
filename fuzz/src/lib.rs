//! Fuzz bodies for SunMao's untrusted-input paths.
//!
//! These live in a library so the same code is driven two ways: by the stable
//! random driver in `main.rs`, and by `libfuzzer` targets under `fuzz_targets/`
//! for anyone with `cargo-fuzz` installed. Keeping one implementation means the
//! coverage-guided targets cannot silently drift from the code that is actually
//! exercised day to day.
//!
//! What is worth fuzzing here is anything that parses bytes the plugin did not
//! produce. Saved state qualifies: it arrives from a project file or a preset
//! that a user may have edited, truncated, or corrupted, and it is decoded
//! behind a C ABI where a panic is undefined behaviour rather than a clean
//! error.

use std::ffi::c_void;
use std::sync::Arc;
use sunmao_core::params::{ParamDescriptor, ParamKind, Params};
use sunmao_core::prelude::*;

/// A minimal plugin with one parameter, so state blobs have an entry to match.
#[derive(Default)]
pub struct FuzzPlugin {
    params: Arc<FuzzParams>,
}

pub struct FuzzParams {
    level: sunmao_core::params::FloatParam,
}

impl Default for FuzzParams {
    fn default() -> Self {
        Self {
            level: sunmao_core::params::FloatParam::new("level", "Level", 0.5, 0.0, 1.0),
        }
    }
}

impl Params for FuzzParams {
    fn get_normalized(&self, id: &str) -> Option<f32> {
        (id == "level").then(|| self.level.get())
    }

    fn set_normalized(&self, id: &str, value: f32) {
        if id == "level" {
            self.level.set(value);
        }
    }

    fn descriptors(&self) -> Vec<ParamDescriptor> {
        vec![ParamDescriptor {
            id: "level",
            numeric_id: sunmao_core::stable_param_id("level"),
            name: self.level.name,
            default_normalized: 0.5,
            step_count: 0,
            kind: ParamKind::Float,
        }]
    }
}

impl SunmaoPlugin for FuzzPlugin {
    const NAME: &'static str = "SunMao Fuzz";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://example.invalid";
    type Params = FuzzParams;

    fn params(&self) -> Arc<Self::Params> {
        self.params.clone()
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

// ======= CLAP state load =======

struct ByteReader {
    bytes: Vec<u8>,
    position: usize,
}

unsafe extern "C" fn reader_read(
    stream: *const clap_rs::clap_sys::stream::clap_istream_t,
    buffer: *mut c_void,
    size: u64,
) -> i64 {
    let reader = unsafe { &mut *((*stream).ctx as *mut ByteReader) };
    let remaining = reader.bytes.len() - reader.position;
    let count = remaining.min(size as usize);
    unsafe {
        std::ptr::copy_nonoverlapping(
            reader.bytes[reader.position..].as_ptr(),
            buffer.cast::<u8>(),
            count,
        );
    }
    reader.position += count;
    count as i64
}

/// Feeds arbitrary bytes to a real CLAP plugin's `clap.state` load.
///
/// The contract being fuzzed: any byte sequence is either rejected or applied,
/// but never panics across the C ABI and never reads out of bounds.
pub fn fuzz_clap_state_load(data: &[u8]) {
    use sunmao_backend_clap::SunmaoClapWrapper;

    let plugin = unsafe {
        clap_rs::entry::PluginEntry::create_plugin::<SunmaoClapWrapper<FuzzPlugin>>(
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if plugin.is_null() {
        return;
    }
    unsafe {
        if !((*plugin).init.unwrap())(plugin) {
            ((*plugin).destroy.unwrap())(plugin);
            return;
        }
        let ext = ((*plugin).get_extension.unwrap())(
            plugin,
            clap_rs::clap_sys::ext::state::CLAP_EXT_STATE
                .as_ptr()
                .cast(),
        ) as *const clap_rs::clap_sys::ext::state::clap_plugin_state_t;
        if !ext.is_null() {
            let mut reader = ByteReader {
                bytes: data.to_vec(),
                position: 0,
            };
            let stream = clap_rs::clap_sys::stream::clap_istream_t {
                ctx: (&mut reader as *mut ByteReader).cast::<c_void>(),
                read: Some(reader_read),
            };
            let _ = ((*ext).load.unwrap())(plugin, &stream);
        }
        ((*plugin).destroy.unwrap())(plugin);
    }
}

// ======= VST3 state load =======

#[repr(C)]
struct ByteStream {
    vtbl: *const vst3_rs::vst3_sys::base::ibstream::IBStreamVtbl,
    bytes: Vec<u8>,
    position: usize,
}

unsafe extern "system" fn stream_query(
    _this: *mut c_void,
    _iid: *const vst3_rs::vst3_sys::base::TUID,
    object: *mut *mut c_void,
) -> i32 {
    if !object.is_null() {
        unsafe { *object = std::ptr::null_mut() };
    }
    vst3_rs::vst3_sys::base::kNoInterface
}

unsafe extern "system" fn stream_add_ref(_this: *mut c_void) -> u32 {
    1
}

unsafe extern "system" fn stream_release(_this: *mut c_void) -> u32 {
    1
}

unsafe extern "system" fn stream_read(
    this: *mut c_void,
    buffer: *mut c_void,
    num_bytes: i32,
    num_bytes_read: *mut i32,
) -> i32 {
    if this.is_null() || buffer.is_null() || num_bytes < 0 {
        return vst3_rs::vst3_sys::base::kInvalidArgument;
    }
    let stream = unsafe { &mut *(this as *mut ByteStream) };
    let remaining = stream.bytes.len() - stream.position;
    let count = remaining.min(num_bytes as usize);
    unsafe {
        std::ptr::copy_nonoverlapping(
            stream.bytes[stream.position..].as_ptr(),
            buffer.cast::<u8>(),
            count,
        );
    }
    stream.position += count;
    if !num_bytes_read.is_null() {
        unsafe { *num_bytes_read = count as i32 };
    }
    vst3_rs::vst3_sys::base::kResultOk
}

unsafe extern "system" fn stream_write(
    _this: *mut c_void,
    _buffer: *mut c_void,
    _num_bytes: i32,
    _num_bytes_written: *mut i32,
) -> i32 {
    vst3_rs::vst3_sys::base::kNotImplemented
}

unsafe extern "system" fn stream_seek(
    _this: *mut c_void,
    _pos: i64,
    _mode: i32,
    _result: *mut i64,
) -> i32 {
    vst3_rs::vst3_sys::base::kNotImplemented
}

unsafe extern "system" fn stream_tell(_this: *mut c_void, _pos: *mut i64) -> i32 {
    vst3_rs::vst3_sys::base::kNotImplemented
}

static BYTE_STREAM_VTBL: vst3_rs::vst3_sys::base::ibstream::IBStreamVtbl =
    vst3_rs::vst3_sys::base::ibstream::IBStreamVtbl {
        unknown: vst3_rs::vst3_sys::base::IUnknownVtbl {
            query_interface: stream_query,
            add_ref: stream_add_ref,
            release: stream_release,
        },
        read: stream_read,
        write: stream_write,
        seek: stream_seek,
        tell: stream_tell,
    };

/// Feeds arbitrary bytes to a real VST3 component's `setState`.
pub fn fuzz_vst3_state_load(data: &[u8]) {
    use sunmao_backend_vst3::SunmaoVst3Wrapper;
    use vst3_rs::vst3_sys::vst::IComponentVtbl;

    unsafe {
        let processor =
            vst3_rs::wrapper::ProcessorWrapper::<SunmaoVst3Wrapper<FuzzPlugin>>::new([0; 16]);
        if processor.is_null() {
            return;
        }
        let component = processor.cast::<c_void>();
        let vtbl = *(component as *const *const IComponentVtbl);
        if ((*vtbl).base.initialize)(component, std::ptr::null_mut())
            == vst3_rs::vst3_sys::base::kResultOk
        {
            let mut stream = ByteStream {
                vtbl: &BYTE_STREAM_VTBL,
                bytes: data.to_vec(),
                position: 0,
            };
            let _ =
                ((*vtbl).set_state)(component, (&mut stream as *mut ByteStream).cast::<c_void>());
            ((*vtbl).base.terminate)(component);
        }
        ((*vtbl).base.unknown.release)(component);
    }
}
