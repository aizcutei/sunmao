#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_audio_buffer_t {
    pub data32: *mut *mut f32,
    pub data64: *mut *mut f64,
    pub channel_count: u32,
    pub latency: u32,
    pub constant_mask: u64,
}
