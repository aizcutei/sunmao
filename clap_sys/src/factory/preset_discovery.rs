use crate::timestamp::clap_timestamp;
use crate::universal_plugin_id::clap_universal_plugin_id_t;
use crate::version::clap_version_t;
use std::ffi::{c_char, c_void};

pub const CLAP_PRESET_DISCOVERY_FACTORY_ID: &str = "clap.preset-discovery-factory/2\0";
pub const CLAP_PRESET_DISCOVERY_FACTORY_ID_COMPAT: &str = "clap.preset-discovery-factory/draft-2\0";

pub const CLAP_PRESET_DISCOVERY_LOCATION_FILE: u32 = 0;
pub const CLAP_PRESET_DISCOVERY_LOCATION_PLUGIN: u32 = 1;

pub const CLAP_PRESET_DISCOVERY_IS_FACTORY_CONTENT: u32 = 1 << 0;
pub const CLAP_PRESET_DISCOVERY_IS_USER_CONTENT: u32 = 1 << 1;
pub const CLAP_PRESET_DISCOVERY_IS_DEMO_CONTENT: u32 = 1 << 2;
pub const CLAP_PRESET_DISCOVERY_IS_FAVORITE: u32 = 1 << 3;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_preset_discovery_metadata_receiver_t {
    pub receiver_data: *mut c_void,
    pub on_error: Option<
        unsafe extern "C" fn(
            receiver: *const clap_preset_discovery_metadata_receiver_t,
            os_error: i32,
            error_message: *const c_char,
        ),
    >,
    pub begin_preset: Option<
        unsafe extern "C" fn(
            receiver: *const clap_preset_discovery_metadata_receiver_t,
            name: *const c_char,
            load_key: *const c_char,
        ) -> bool,
    >,
    pub add_plugin_id: Option<
        unsafe extern "C" fn(
            receiver: *const clap_preset_discovery_metadata_receiver_t,
            plugin_id: *const clap_universal_plugin_id_t,
        ),
    >,
    pub set_soundpack_id: Option<
        unsafe extern "C" fn(
            receiver: *const clap_preset_discovery_metadata_receiver_t,
            soundpack_id: *const c_char,
        ),
    >,
    pub set_flags: Option<
        unsafe extern "C" fn(
            receiver: *const clap_preset_discovery_metadata_receiver_t,
            flags: u32,
        ),
    >,
    pub add_creator: Option<
        unsafe extern "C" fn(
            receiver: *const clap_preset_discovery_metadata_receiver_t,
            creator: *const c_char,
        ),
    >,
    pub set_description: Option<
        unsafe extern "C" fn(
            receiver: *const clap_preset_discovery_metadata_receiver_t,
            description: *const c_char,
        ),
    >,
    pub set_timestamps: Option<
        unsafe extern "C" fn(
            receiver: *const clap_preset_discovery_metadata_receiver_t,
            creation_time: clap_timestamp,
            modification_time: clap_timestamp,
        ),
    >,
    pub add_feature: Option<
        unsafe extern "C" fn(
            receiver: *const clap_preset_discovery_metadata_receiver_t,
            feature: *const c_char,
        ),
    >,
    pub add_extra_info: Option<
        unsafe extern "C" fn(
            receiver: *const clap_preset_discovery_metadata_receiver_t,
            key: *const c_char,
            value: *const c_char,
        ),
    >,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_preset_discovery_filetype_t {
    pub name: *const c_char,
    pub description: *const c_char,
    pub file_extension: *const c_char,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_preset_discovery_location_t {
    pub flags: u32,
    pub name: *const c_char,
    pub kind: u32,
    pub location: *const c_char,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_preset_discovery_soundpack_t {
    pub flags: u32,
    pub id: *const c_char,
    pub name: *const c_char,
    pub description: *const c_char,
    pub homepage_url: *const c_char,
    pub vendor: *const c_char,
    pub image_path: *const c_char,
    pub release_timestamp: clap_timestamp,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_preset_discovery_provider_descriptor_t {
    pub clap_version: clap_version_t,
    pub id: *const c_char,
    pub name: *const c_char,
    pub vendor: *const c_char,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_preset_discovery_provider_t {
    pub desc: *const clap_preset_discovery_provider_descriptor_t,
    pub provider_data: *mut c_void,
    pub init:
        Option<unsafe extern "C" fn(provider: *const clap_preset_discovery_provider_t) -> bool>,
    pub destroy: Option<unsafe extern "C" fn(provider: *const clap_preset_discovery_provider_t)>,
    pub get_metadata: Option<
        unsafe extern "C" fn(
            provider: *const clap_preset_discovery_provider_t,
            location_kind: u32,
            location: *const c_char,
            metadata_receiver: *const clap_preset_discovery_metadata_receiver_t,
        ) -> bool,
    >,
    pub get_extension: Option<
        unsafe extern "C" fn(
            provider: *const clap_preset_discovery_provider_t,
            extension_id: *const c_char,
        ) -> *const c_void,
    >,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_preset_discovery_indexer_t {
    pub clap_version: clap_version_t,
    pub name: *const c_char,
    pub vendor: *const c_char,
    pub url: *const c_char,
    pub version: *const c_char,
    pub indexer_data: *mut c_void,
    pub declare_filetype: Option<
        unsafe extern "C" fn(
            indexer: *const clap_preset_discovery_indexer_t,
            filetype: *const clap_preset_discovery_filetype_t,
        ) -> bool,
    >,
    pub declare_location: Option<
        unsafe extern "C" fn(
            indexer: *const clap_preset_discovery_indexer_t,
            location: *const clap_preset_discovery_location_t,
        ) -> bool,
    >,
    pub declare_soundpack: Option<
        unsafe extern "C" fn(
            indexer: *const clap_preset_discovery_indexer_t,
            soundpack: *const clap_preset_discovery_soundpack_t,
        ) -> bool,
    >,
    pub get_extension: Option<
        unsafe extern "C" fn(
            indexer: *const clap_preset_discovery_indexer_t,
            extension_id: *const c_char,
        ) -> *const c_void,
    >,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_preset_discovery_factory_t {
    pub count: Option<unsafe extern "C" fn(factory: *const clap_preset_discovery_factory_t) -> u32>,
    pub get_descriptor: Option<
        unsafe extern "C" fn(
            factory: *const clap_preset_discovery_factory_t,
            index: u32,
        ) -> *const clap_preset_discovery_provider_descriptor_t,
    >,
    pub create: Option<
        unsafe extern "C" fn(
            factory: *const clap_preset_discovery_factory_t,
            indexer: *const clap_preset_discovery_indexer_t,
            provider_id: *const c_char,
        ) -> *const clap_preset_discovery_provider_t,
    >,
}
