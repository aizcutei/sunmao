#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_color_t {
    pub alpha: u8,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

pub const CLAP_COLOR_TRANSPARENT: clap_color_t = clap_color_t {
    alpha: 0,
    red: 0,
    green: 0,
    blue: 0,
};
