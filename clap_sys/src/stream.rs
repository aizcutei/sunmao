use std::ffi::c_void;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_istream_t {
    pub ctx: *mut c_void,
    pub read: Option<
        unsafe extern "C" fn(stream: *const clap_istream_t, buffer: *mut c_void, size: u64) -> i64,
    >,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_ostream_t {
    pub ctx: *mut c_void,
    pub write: Option<
        unsafe extern "C" fn(
            stream: *const clap_ostream_t,
            buffer: *const c_void,
            size: u64,
        ) -> i64,
    >,
}
