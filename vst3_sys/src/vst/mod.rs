//! VST module - audio effect and instrument interfaces

pub mod iaudioprocessor;
pub mod icomponent;
pub mod ieditcontroller;
pub mod ievents;
pub mod iparameters;
pub mod ivstmessage;
pub mod processcontext;
pub mod types;

pub use iaudioprocessor::*;
pub use icomponent::*;
pub use ieditcontroller::*;
pub use ievents::*;
pub use iparameters::*;
pub use ivstmessage::*;
pub use processcontext::*;
pub use types::*;

// =============================================================================
// Interface IIDs
// =============================================================================

pub mod iid {
    use crate::base::types::TUID;
    use crate::uid;

    pub const IComponent: TUID = uid!(0xE831FF31, 0xF2D54301, 0x928EBBEE, 0x25697802);
    pub const IAudioProcessor: TUID = uid!(0x42043F99, 0xB7DA453C, 0xA569E79D, 0x9AAEC33D);
    pub const IEditController: TUID = uid!(0xDCD7BBE3, 0x7742448D, 0xA874AACC, 0x979C759E);
    pub const IProcessContextRequirements: TUID =
        uid!(0x2A654303, 0xEF764E3D, 0x95B5FE83, 0x730EF6D0);
    pub const IEventList: TUID = uid!(0x3A2C4214, 0x346349FE, 0xB2C4F397, 0xB9695A44);
    pub const IConnectionPoint: TUID = uid!(0x70A4156F, 0x6E6E4026, 0x989148BF, 0xAA60D8D1);
    pub const IParameterChanges: TUID = uid!(0xA4779663, 0x0BB64A56, 0xB44384A8, 0x466FEB9D);
    pub const IParamValueQueue: TUID = uid!(0x01263A18, 0xED074F6F, 0x98C9D356, 0x4686F9BA);
    pub const IComponentHandler: TUID = uid!(0x93A0BEA3, 0x0BD045DB, 0x8E890B0C, 0xC1E46AC6);
    pub const IComponentHandler2: TUID = uid!(0xF040B4B3, 0xA36045EC, 0xABCDC045, 0xB4D5A2CC);
    pub const IMidiMapping: TUID = uid!(0xDF0FF9F7, 0x49B74669, 0xB63AB732, 0x7ADBF5E5);
}
