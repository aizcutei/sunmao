//! IPluginBase and IPluginFactory interfaces

use std::ffi::c_void;
use crate::base::types::*;

// =============================================================================
// Factory Info Structs
// =============================================================================

pub mod PFactoryInfo {
    pub const kURLSize: usize = 256;
    pub const kEmailSize: usize = 128;
    pub const kNameSize: usize = 64;

    pub mod Flags {
        use crate::base::types::int32;
        pub const kNoFlags: int32 = 0;
        pub const kClassesDiscardable: int32 = 1 << 0;
        pub const kLicenseCheck: int32 = 1 << 1;
        pub const kComponentNonDiscardable: int32 = 1 << 3;
        pub const kUnicode: int32 = 1 << 4;
    }
}

#[repr(C)]
pub struct PFactoryInfoData {
    pub vendor: [char8; PFactoryInfo::kNameSize],
    pub url: [char8; PFactoryInfo::kURLSize],
    pub email: [char8; PFactoryInfo::kEmailSize],
    pub flags: int32,
}

pub mod PClassInfo {
    pub const kCategorySize: usize = 32;
    pub const kNameSize: usize = 64;
    pub const kManyInstances: i32 = 0x7FFF_FFFF;
}

#[repr(C)]
pub struct PClassInfoData {
    pub cid: TUID,
    pub cardinality: int32,
    pub category: [char8; PClassInfo::kCategorySize],
    pub name: [char8; PClassInfo::kNameSize],
}

pub mod PClassInfo2 {
    pub const kVendorSize: usize = 64;
    pub const kVersionSize: usize = 64;
    pub const kSubCategoriesSize: usize = 128;
}

#[repr(C)]
pub struct PClassInfo2Data {
    pub cid: TUID,
    pub cardinality: int32,
    pub category: [char8; PClassInfo::kCategorySize],
    pub name: [char8; PClassInfo::kNameSize],
    pub class_flags: uint32,
    pub sub_categories: [char8; PClassInfo2::kSubCategoriesSize],
    pub vendor: [char8; PClassInfo2::kVendorSize],
    pub version: [char8; PClassInfo2::kVersionSize],
    pub sdk_version: [char8; PClassInfo2::kVersionSize],
}

pub mod PClassInfoW {
    pub const kVendorSize: usize = 64;
    pub const kVersionSize: usize = 64;
}

#[repr(C)]
pub struct PClassInfoWData {
    pub cid: TUID,
    pub cardinality: int32,
    pub category: [char8; PClassInfo::kCategorySize],
    pub name: [char16; PClassInfo::kNameSize],
    pub class_flags: uint32,
    pub sub_categories: [char8; PClassInfo2::kSubCategoriesSize],
    pub vendor: [char16; PClassInfoW::kVendorSize],
    pub version: [char16; PClassInfoW::kVersionSize],
    pub sdk_version: [char16; PClassInfoW::kVersionSize],
}

// =============================================================================
// IPluginBase VTable
// =============================================================================

/// IPluginBase vtable - basic plugin lifecycle
#[repr(C)]
pub struct IPluginBaseVtbl {
    pub unknown: IUnknownVtbl,
    pub initialize: unsafe extern "system" fn(this: *mut c_void, context: *mut c_void) -> tresult,
    pub terminate: unsafe extern "system" fn(this: *mut c_void) -> tresult,
}

// =============================================================================
// IPluginFactory VTables
// =============================================================================

/// IPluginFactory vtable
#[repr(C)]
pub struct IPluginFactoryVtbl {
    pub unknown: IUnknownVtbl,
    pub get_factory_info: unsafe extern "system" fn(this: *mut c_void, info: *mut PFactoryInfoData) -> tresult,
    pub count_classes: unsafe extern "system" fn(this: *mut c_void) -> int32,
    pub get_class_info: unsafe extern "system" fn(this: *mut c_void, index: int32, info: *mut PClassInfoData) -> tresult,
    pub create_instance: unsafe extern "system" fn(
        this: *mut c_void,
        cid: FIDString,
        iid: FIDString,
        obj: *mut *mut c_void,
    ) -> tresult,
}

/// IPluginFactory2 vtable (extends IPluginFactory)
#[repr(C)]
pub struct IPluginFactory2Vtbl {
    pub factory: IPluginFactoryVtbl,
    pub get_class_info2: unsafe extern "system" fn(this: *mut c_void, index: int32, info: *mut PClassInfo2Data) -> tresult,
}

/// IPluginFactory3 vtable (extends IPluginFactory2)
#[repr(C)]
pub struct IPluginFactory3Vtbl {
    pub factory2: IPluginFactory2Vtbl,
    pub get_class_info_unicode: unsafe extern "system" fn(this: *mut c_void, index: int32, info: *mut PClassInfoWData) -> tresult,
    pub set_host_context: unsafe extern "system" fn(this: *mut c_void, context: *mut c_void) -> tresult,
}
