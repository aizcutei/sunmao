use crate::ParamInfo;
use std::ffi::c_void;
use vst3_sys::base::{IBStreamVtbl, int32, kInvalidArgument, kResultFalse, kResultOk, tresult};

const STATE_MAGIC: [u8; 8] = *b"SMV3PRM\0";
const STATE_VERSION: u32 = 1;
const HEADER_LEN: usize = 16;
const ENTRY_LEN: usize = 12;
const MAX_STATE_PARAMETERS: usize = 65_536;

pub(crate) unsafe fn save_parameter_state(
    stream: *mut c_void,
    params: &[ParamInfo],
    value_for: impl FnMut(u32) -> f64,
) -> tresult {
    if stream.is_null() {
        return kInvalidArgument;
    }
    let Some(bytes) = encode_parameter_state(params, value_for) else {
        return kResultFalse;
    };
    if unsafe { stream_write_all(stream, &bytes) } {
        kResultOk
    } else {
        kResultFalse
    }
}

pub(crate) unsafe fn load_parameter_state(
    stream: *mut c_void,
    params: &[ParamInfo],
    mut apply: impl FnMut(u32, f64),
) -> tresult {
    if stream.is_null() {
        return kInvalidArgument;
    }

    let mut header = [0u8; HEADER_LEN];
    if !unsafe { stream_read_exact(stream, &mut header) } {
        return kResultFalse;
    }
    let Some(count) = decode_header(&header) else {
        return kResultFalse;
    };
    let Some(body_len) = count.checked_mul(ENTRY_LEN) else {
        return kResultFalse;
    };
    let mut bytes = Vec::with_capacity(HEADER_LEN + body_len);
    bytes.extend_from_slice(&header);
    bytes.resize(HEADER_LEN + body_len, 0);
    if !unsafe { stream_read_exact(stream, &mut bytes[HEADER_LEN..]) } {
        return kResultFalse;
    }

    let Some(entries) = decode_parameter_state(&bytes) else {
        return kResultFalse;
    };
    for (id, value) in entries {
        if params.iter().any(|param| param.id == id) {
            apply(id, value);
        }
    }
    kResultOk
}

fn encode_parameter_state(
    params: &[ParamInfo],
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
        if !value.is_finite() {
            return None;
        }
        bytes.extend_from_slice(&param.id.to_le_bytes());
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    Some(bytes)
}

fn decode_header(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < HEADER_LEN || bytes[..8] != STATE_MAGIC {
        return None;
    }
    let version = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
    if version != STATE_VERSION {
        return None;
    }
    let count = u32::from_le_bytes(bytes[12..16].try_into().ok()?) as usize;
    (count <= MAX_STATE_PARAMETERS).then_some(count)
}

fn decode_parameter_state(bytes: &[u8]) -> Option<Vec<(u32, f64)>> {
    let count = decode_header(bytes)?;
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

unsafe fn stream_write_all(stream: *mut c_void, mut bytes: &[u8]) -> bool {
    let vtbl = unsafe { *(stream as *const *const IBStreamVtbl) };
    if vtbl.is_null() {
        return false;
    }
    while !bytes.is_empty() {
        let requested = bytes.len().min(int32::MAX as usize) as int32;
        let mut written = 0;
        let result = unsafe {
            ((*vtbl).write)(
                stream,
                bytes.as_ptr() as *mut c_void,
                requested,
                &mut written,
            )
        };
        if result != kResultOk || written <= 0 || written > requested {
            return false;
        }
        bytes = &bytes[written as usize..];
    }
    true
}

unsafe fn stream_read_exact(stream: *mut c_void, mut bytes: &mut [u8]) -> bool {
    let vtbl = unsafe { *(stream as *const *const IBStreamVtbl) };
    if vtbl.is_null() {
        return false;
    }
    while !bytes.is_empty() {
        let requested = bytes.len().min(int32::MAX as usize) as int32;
        let mut read = 0;
        let result = unsafe {
            ((*vtbl).read)(
                stream,
                bytes.as_mut_ptr() as *mut c_void,
                requested,
                &mut read,
            )
        };
        if result != kResultOk || read <= 0 || read > requested {
            return false;
        }
        bytes = &mut bytes[read as usize..];
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> Vec<ParamInfo> {
        vec![ParamInfo::new(7, "Gain"), ParamInfo::new(42, "Mode")]
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

        let reordered = vec![ParamInfo::new(42, "Mode"), ParamInfo::new(7, "Gain")];
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
}
