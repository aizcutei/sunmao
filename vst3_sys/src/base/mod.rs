//! Base module - fundamental types and interfaces

pub mod types;
pub mod ipluginbase;
pub mod ibstream;

pub use types::*;
pub use ipluginbase::*;
pub use ibstream::*;

// =============================================================================
// Interface IIDs
// =============================================================================

pub mod iid {
    use super::types::TUID;
    use crate::uid;

    pub const IUnknown: TUID = uid!(0x00000000, 0x00000000, 0xC0000000, 0x00000046);
    pub const IPluginBase: TUID = uid!(0x22888DDB, 0x156E45AE, 0x8358B348, 0x08190625);
    pub const IPluginFactory: TUID = uid!(0x7A4D811C, 0x52114A1F, 0xAED9D2EE, 0x0B43BF9F);
    pub const IPluginFactory2: TUID = uid!(0x0007B650, 0xF24B4C0B, 0xA464EDB9, 0xF00B2ABB);
    pub const IPluginFactory3: TUID = uid!(0x4555A2AB, 0xC1234E57, 0x9B122910, 0x36878931);
    pub const IBStream: TUID = uid!(0xC3BF6EA2, 0x30994752, 0x9B6BF990, 0x1EE33E9B);
}
