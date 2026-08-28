//! CLAP state extension.

use crate::ext::gui::GuiHandler;
use crate::ext::params::ParameterInfo;
use crate::plugin::Plugin;
use crate::plugin_instance::{ffi_guard, instance_ptr};
use clap_sys::ext::state::clap_plugin_state_t;
use clap_sys::plugin::clap_plugin_t;
use clap_sys::stream::{clap_istream_t, clap_ostream_t};

const STATE_MAGIC: [u8; 8] = *b"SMCLPRM\0";
const STATE_VERSION: u32 = 1;
const HEADER_LEN: usize = 16;
const ENTRY_LEN: usize = 12;
const MAX_STATE_PARAMETERS: usize = 65_536;

pub(crate) unsafe extern "C" fn state_save<P: Plugin>(
    plugin: *const clap_plugin_t,
    stream: *const clap_ostream_t,
) -> bool {
    ffi_guard(false, || unsafe {
        state_save_unchecked::<P>(plugin, stream)
    })
}

unsafe fn state_save_unchecked<P: Plugin>(
    plugin: *const clap_plugin_t,
    stream: *const clap_ostream_t,
) -> bool {
    if stream.is_null() {
        return false;
    }
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &*instance_ptr };
    unsafe { save_parameter_state(instance.controller(), &instance.params_cache, stream) }
}

pub(crate) unsafe extern "C" fn state_load<P: Plugin>(
    plugin: *const clap_plugin_t,
    stream: *const clap_istream_t,
) -> bool {
    ffi_guard(false, || unsafe {
        state_load_unchecked::<P>(plugin, stream)
    })
}

unsafe fn state_load_unchecked<P: Plugin>(
    plugin: *const clap_plugin_t,
    stream: *const clap_istream_t,
) -> bool {
    if stream.is_null() {
        return false;
    }
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &*instance_ptr };
    let loaded =
        unsafe { load_parameter_state(instance.controller_mut(), &instance.params_cache, stream) };
    if loaded {
        unsafe { instance.refresh_tail_cache() };
    }
    loaded
}

/// Create the state extension for a plugin without a GUI.
pub(crate) fn create_state_ext<P: Plugin>() -> clap_plugin_state_t {
    clap_plugin_state_t {
        save: Some(state_save::<P>),
        load: Some(state_load::<P>),
    }
}

pub(crate) unsafe extern "C" fn state_save_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    stream: *const clap_ostream_t,
) -> bool {
    ffi_guard(false, || unsafe {
        state_save_gui_unchecked::<P>(plugin, stream)
    })
}

unsafe fn state_save_gui_unchecked<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    stream: *const clap_ostream_t,
) -> bool {
    if stream.is_null() {
        return false;
    }
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &*instance_ptr };
    unsafe { save_parameter_state(instance.controller(), &instance.params_cache, stream) }
}

pub(crate) unsafe extern "C" fn state_load_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    stream: *const clap_istream_t,
) -> bool {
    ffi_guard(false, || unsafe {
        state_load_gui_unchecked::<P>(plugin, stream)
    })
}

unsafe fn state_load_gui_unchecked<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    stream: *const clap_istream_t,
) -> bool {
    if stream.is_null() {
        return false;
    }
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &*instance_ptr };
    let loaded =
        unsafe { load_parameter_state(instance.controller_mut(), &instance.params_cache, stream) };
    if loaded {
        unsafe { instance.refresh_tail_cache() };
    }
    loaded
}

/// Create the state extension for a plugin with a GUI.
pub(crate) fn create_state_ext_gui<P: Plugin + GuiHandler>() -> clap_plugin_state_t {
    clap_plugin_state_t {
        save: Some(state_save_gui::<P>),
        load: Some(state_load_gui::<P>),
    }
}

unsafe fn save_parameter_state<P: Plugin>(
    plugin: &P,
    params: &[ParameterInfo],
    stream: *const clap_ostream_t,
) -> bool {
    let Some(bytes) = encode_parameter_state(params, |id| plugin.get_parameter(id)) else {
        return false;
    };
    unsafe { stream_write_all(stream, &bytes) }
}

unsafe fn load_parameter_state<P: Plugin>(
    plugin: &mut P,
    params: &[ParameterInfo],
    stream: *const clap_istream_t,
) -> bool {
    let mut header = [0u8; HEADER_LEN];
    if !unsafe { stream_read_exact(stream, &mut header) } {
        return false;
    }
    let Some((_version, count)) = decode_header(&header) else {
        return false;
    };
    let Some(body_len) = count.checked_mul(ENTRY_LEN) else {
        return false;
    };
    let mut bytes = Vec::with_capacity(HEADER_LEN + body_len);
    bytes.extend_from_slice(&header);
    bytes.resize(HEADER_LEN + body_len, 0);
    if !unsafe { stream_read_exact(stream, &mut bytes[HEADER_LEN..]) } {
        return false;
    }

    let Some(entries) = decode_parameter_state(&bytes) else {
        return false;
    };
    // CLAP parameter state is stored in the parameter's plain-value domain.
    // Reject finite-but-invalid values before exposing *any* entry to plugin
    // code, keeping a malformed state load atomic from the plugin's point of
    // view.
    for (id, value) in &entries {
        if params.iter().any(|param| param.id == *id) && !state_value_valid(params, *id, *value) {
            return false;
        }
    }
    for (id, value) in entries {
        if params.iter().any(|param| param.id == id) {
            plugin.set_parameter(id, value);
        }
    }
    true
}

fn state_value_valid(params: &[ParameterInfo], id: u32, value: f64) -> bool {
    let Some(param) = params.iter().find(|param| param.id == id) else {
        return true;
    };
    parameter_value_valid(param, value)
}

fn parameter_value_valid(param: &ParameterInfo, value: f64) -> bool {
    value.is_finite()
        && param.min_value.is_finite()
        && param.max_value.is_finite()
        && param.min_value <= param.max_value
        && (param.min_value..=param.max_value).contains(&value)
}

fn encode_parameter_state(
    params: &[ParameterInfo],
    mut value_for: impl FnMut(u32) -> f64,
) -> Option<Vec<u8>> {
    if params.len() > MAX_STATE_PARAMETERS {
        return None;
    }
    let count = u32::try_from(params.len()).ok()?;
    let mut bytes = Vec::with_capacity(HEADER_LEN + params.len() * ENTRY_LEN);
    bytes.extend_from_slice(&STATE_MAGIC);
    bytes.extend_from_slice(&STATE_VERSION.to_le_bytes());
    bytes.extend_from_slice(&count.to_le_bytes());
    for param in params {
        let value = value_for(param.id);
        if !parameter_value_valid(param, value) {
            return None;
        }
        bytes.extend_from_slice(&param.id.to_le_bytes());
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    Some(bytes)
}

/// Reads the header, returning `(version, entry count)`.
///
/// A state written by an older build is accepted: entries are matched by
/// parameter id, so parameters that did not exist yet simply keep their
/// defaults. A *newer* version is rejected because this build cannot know how
/// to interpret it.
fn decode_header(bytes: &[u8]) -> Option<(u32, usize)> {
    if bytes.len() < HEADER_LEN || bytes[..8] != STATE_MAGIC {
        return None;
    }
    let version = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
    if version > STATE_VERSION {
        return None;
    }
    let count = u32::from_le_bytes(bytes[12..16].try_into().ok()?) as usize;
    (count <= MAX_STATE_PARAMETERS).then_some((version, count))
}

fn decode_parameter_state(bytes: &[u8]) -> Option<Vec<(u32, f64)>> {
    let (_version, count) = decode_header(bytes)?;
    let required_len = HEADER_LEN.checked_add(count.checked_mul(ENTRY_LEN)?)?;
    if bytes.len() < required_len {
        return None;
    }

    let mut entries = Vec::with_capacity(count);
    for entry in bytes[HEADER_LEN..required_len].chunks_exact(ENTRY_LEN) {
        let id = u32::from_le_bytes(entry[..4].try_into().ok()?);
        let value = f64::from_le_bytes(entry[4..].try_into().ok()?);
        if !value.is_finite() {
            return None;
        }
        entries.push((id, value));
    }
    Some(entries)
}

unsafe fn stream_write_all(stream: *const clap_ostream_t, mut bytes: &[u8]) -> bool {
    if stream.is_null() {
        return false;
    }
    let Some(write) = (unsafe { (*stream).write }) else {
        return false;
    };
    while !bytes.is_empty() {
        let requested = bytes.len().min(u64::MAX as usize);
        let written = unsafe { write(stream, bytes.as_ptr().cast(), requested as u64) };
        let Ok(written) = usize::try_from(written) else {
            return false;
        };
        if written == 0 || written > requested {
            return false;
        }
        bytes = &bytes[written..];
    }
    true
}

unsafe fn stream_read_exact(stream: *const clap_istream_t, mut bytes: &mut [u8]) -> bool {
    if stream.is_null() {
        return false;
    }
    let Some(read) = (unsafe { (*stream).read }) else {
        return false;
    };
    while !bytes.is_empty() {
        let requested = bytes.len().min(u64::MAX as usize);
        let count = unsafe { read(stream, bytes.as_mut_ptr().cast(), requested as u64) };
        let Ok(count) = usize::try_from(count) else {
            return false;
        };
        if count == 0 || count > requested {
            return false;
        }
        bytes = &mut bytes[count..];
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parameter(id: u32, name: &str) -> ParameterInfo {
        ParameterInfo {
            id,
            name: name.to_string(),
            module: String::new(),
            min_value: 0.0,
            max_value: 1.0,
            default_value: 0.0,
            is_stepped: false,
        }
    }

    fn params() -> Vec<ParameterInfo> {
        vec![parameter(7, "Gain"), parameter(42, "Mode")]
    }

    #[test]
    fn state_is_versioned_and_keyed_by_parameter_id() {
        let bytes = encode_parameter_state(&params(), |id| match id {
            7 => 0.25,
            42 => 1.0,
            _ => unreachable!(),
        })
        .unwrap();
        assert_eq!(
            decode_parameter_state(&bytes),
            Some(vec![(7, 0.25), (42, 1.0)])
        );

        let reordered = [parameter(42, "Mode"), parameter(7, "Gain")];
        let restored = decode_parameter_state(&bytes).unwrap();
        assert_eq!(
            reordered
                .iter()
                .map(|param| restored.iter().find(|entry| entry.0 == param.id).unwrap().1)
                .collect::<Vec<_>>(),
            vec![1.0, 0.25]
        );
    }

    #[test]
    fn malformed_state_is_rejected_before_values_are_exposed() {
        let mut bytes = encode_parameter_state(&params(), |_| 0.5).unwrap();
        bytes.truncate(bytes.len() - 1);
        assert_eq!(decode_parameter_state(&bytes), None);

        let mut bytes = encode_parameter_state(&params(), |_| 0.5).unwrap();
        bytes[8..12].copy_from_slice(&99u32.to_le_bytes());
        assert_eq!(decode_parameter_state(&bytes), None);

        let mut bytes = encode_parameter_state(&params(), |_| 0.5).unwrap();
        bytes[HEADER_LEN + 4..HEADER_LEN + ENTRY_LEN].copy_from_slice(&f64::NAN.to_le_bytes());
        assert_eq!(decode_parameter_state(&bytes), None);
    }

    #[test]
    fn out_of_range_state_values_are_rejected() {
        assert!(!state_value_valid(&params(), 7, 2.0));
        assert!(!state_value_valid(&params(), 7, f64::NAN));
        assert!(state_value_valid(&params(), 999, 2.0));
    }

    fn header_bytes(version: u32, count: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&STATE_MAGIC);
        bytes.extend_from_slice(&version.to_le_bytes());
        bytes.extend_from_slice(&count.to_le_bytes());
        bytes
    }

    #[test]
    fn a_state_from_an_older_build_is_accepted() {
        // Entries are matched by parameter id, so an older layout restores
        // what it knew and leaves newer parameters at their defaults. This
        // must not be rejected outright.
        let header = header_bytes(STATE_VERSION.saturating_sub(1).max(0), 0);
        assert_eq!(
            decode_header(&header),
            Some((STATE_VERSION.saturating_sub(1).max(0), 0))
        );
    }

    #[test]
    fn a_state_from_a_newer_build_is_rejected() {
        // This build cannot know how a future layout reinterprets values.
        let header = header_bytes(STATE_VERSION + 1, 0);
        assert_eq!(decode_header(&header), None);
    }

    #[test]
    fn a_foreign_magic_is_rejected() {
        let mut header = header_bytes(STATE_VERSION, 0);
        header[0] = b'X';
        assert_eq!(decode_header(&header), None);
    }
}
