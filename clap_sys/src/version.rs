

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct clap_version_t {
    pub major: u32,
    pub minor: u32,
    pub revision: u32,
}

pub const CLAP_VERSION_MAJOR: u32 = 1;
pub const CLAP_VERSION_MINOR: u32 = 2;
pub const CLAP_VERSION_REVISION: u32 = 7;

pub const CLAP_VERSION: clap_version_t = clap_version_t {
    major: CLAP_VERSION_MAJOR,
    minor: CLAP_VERSION_MINOR,
    revision: CLAP_VERSION_REVISION,
};

#[inline]
pub const fn clap_version_is_compatible(v: clap_version_t) -> bool {
    v.major >= 1
}
