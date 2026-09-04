use crate::ParamInfo;
use crate::plugin::Plugin;
use std::ffi::c_void;
use vst3_sys::base::{IBStreamVtbl, int32, kInvalidArgument, kResultFalse, kResultOk, tresult};

const STATE_MAGIC: [u8; 8] = *b"SMV3PRM\0";
/// Version used when a caller does not supply the plugin's own. Kept so the
/// header layout stays self-describing for pre-Phase-3 blobs.
const STATE_VERSION: u32 = 1;
const HEADER_LEN: usize = 16;
const ENTRY_LEN: usize = 12;
const MAX_STATE_PARAMETERS: usize = 65_536;

pub(crate) unsafe fn save_parameter_state<P: Plugin>(
    stream: *mut c_void,
    params: &[ParamInfo],
    value_for: impl FnMut(u32) -> f64,
) -> tresult {
    if stream.is_null() {
        return kInvalidArgument;
    }
    let Some(bytes) = encode_parameter_state(P::STATE_VERSION, params, value_for) else {
        return kResultFalse;
    };
    if unsafe { stream_write_all(stream, &bytes) } {
        kResultOk
    } else {
        kResultFalse
    }
}

/// Applies a saved blob and reports the version it was written by through
/// `loaded_version`, so the caller can run a migration when it is older.
pub(crate) unsafe fn load_parameter_state<P: Plugin>(
    stream: *mut c_void,
    params: &[ParamInfo],
    mut apply: impl FnMut(u32, f64),
    loaded_version: &mut Option<u32>,
) -> tresult {
    if stream.is_null() {
        return kInvalidArgument;
    }

    let mut header = [0u8; HEADER_LEN];
    if !unsafe { stream_read_exact(stream, &mut header) } {
        return kResultFalse;
    }
    let Some((version, count)) = decode_header(&header, P::STATE_VERSION) else {
        return kResultFalse;
    };
    *loaded_version = Some(version);
    let Some(body_len) = count.checked_mul(ENTRY_LEN) else {
        return kResultFalse;
    };
    let mut bytes = Vec::with_capacity(HEADER_LEN + body_len);
    bytes.extend_from_slice(&header);
    bytes.resize(HEADER_LEN + body_len, 0);
    if !unsafe { stream_read_exact(stream, &mut bytes[HEADER_LEN..]) } {
        return kResultFalse;
    }

    let Some(entries) = decode_parameter_state(&bytes, P::STATE_VERSION) else {
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
    version: u32,
    params: &[ParamInfo],
    mut value_for: impl FnMut(u32) -> f64,
) -> Option<Vec<u8>> {
    if params.len() > MAX_STATE_PARAMETERS {
        return None;
    }
    let count = u32::try_from(params.len()).ok()?;
    let mut bytes = Vec::with_capacity(HEADER_LEN + params.len() * ENTRY_LEN);
    bytes.extend_from_slice(&STATE_MAGIC);
    bytes.extend_from_slice(&version.to_le_bytes());
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

/// Reads the header, returning `(version, entry count)`.
///
/// A state written by an older build is accepted: entries are matched by
/// parameter id, so parameters that did not exist yet simply keep their
/// defaults. A *newer* version is rejected because this build cannot know how
/// to interpret it.
fn decode_header(bytes: &[u8], current_version: u32) -> Option<(u32, usize)> {
    if bytes.len() < HEADER_LEN || bytes[..8] != STATE_MAGIC {
        return None;
    }
    let version = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
    if version > current_version {
        return None;
    }
    let count = u32::from_le_bytes(bytes[12..16].try_into().ok()?) as usize;
    (count <= MAX_STATE_PARAMETERS).then_some((version, count))
}

fn decode_parameter_state(bytes: &[u8], current_version: u32) -> Option<Vec<(u32, f64)>> {
    let (_version, count) = decode_header(bytes, current_version)?;
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
        let bytes = encode_parameter_state(STATE_VERSION, &params(), |id| match id {
            7 => 0.25,
            42 => 1.0,
            _ => unreachable!(),
        })
        .unwrap();
        assert_eq!(
            decode_parameter_state(&bytes, STATE_VERSION),
            Some(vec![(7, 0.25), (42, 1.0)])
        );

        let reordered = vec![ParamInfo::new(42, "Mode"), ParamInfo::new(7, "Gain")];
        let restored = decode_parameter_state(&bytes, STATE_VERSION).unwrap();
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
        let mut bytes = encode_parameter_state(STATE_VERSION, &params(), |_| 0.5).unwrap();
        bytes.truncate(bytes.len() - 1);
        assert_eq!(decode_parameter_state(&bytes, STATE_VERSION), None);

        let mut bytes = encode_parameter_state(STATE_VERSION, &params(), |_| 0.5).unwrap();
        bytes[8..12].copy_from_slice(&99u32.to_le_bytes());
        assert_eq!(decode_parameter_state(&bytes, STATE_VERSION), None);

        let mut bytes = encode_parameter_state(STATE_VERSION, &params(), |_| 0.5).unwrap();
        bytes[HEADER_LEN + 4..HEADER_LEN + ENTRY_LEN].copy_from_slice(&f64::NAN.to_le_bytes());
        assert_eq!(decode_parameter_state(&bytes, STATE_VERSION), None);
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
            decode_header(&header, STATE_VERSION),
            Some((STATE_VERSION.saturating_sub(1).max(0), 0))
        );
    }

    #[test]
    fn a_state_from_a_newer_build_is_rejected() {
        // This build cannot know how a future layout reinterprets values.
        let header = header_bytes(STATE_VERSION + 1, 0);
        assert_eq!(decode_header(&header, STATE_VERSION), None);
    }

    #[test]
    fn a_foreign_magic_is_rejected() {
        let mut header = header_bytes(STATE_VERSION, 0);
        header[0] = b'X';
        assert_eq!(decode_header(&header, STATE_VERSION), None);
    }

    /// The unit tests above pin the codec on hand-written examples. These pin
    /// the rules `docs/phase3/compatibility.md` promises for *every* blob a
    /// host may hand back — including blobs no build of this framework wrote.
    proptest::proptest! {
        /// A saved value must come back exactly. Users' presets are the one
        /// thing a plugin can never regenerate, so "close enough" is not a
        /// tolerance the codec gets: the round trip is bit-for-bit.
        #[test]
        fn any_parameter_set_round_trips_bit_for_bit(
            entries in proptest::collection::vec(
                (proptest::prelude::any::<u32>(), -1.0e300f64..1.0e300),
                0..24,
            ),
            version in proptest::prelude::any::<u32>(),
        ) {
            // Duplicate ids cannot occur in a real layout (`ParamLayoutError`
            // rejects them upstream), so the property is stated over the
            // deduplicated set.
            let mut seen = std::collections::BTreeMap::new();
            for (id, value) in &entries {
                seen.insert(*id, *value);
            }
            let params: Vec<ParamInfo> = seen
                .keys()
                .map(|id| ParamInfo::new(*id, "param"))
                .collect();

            let bytes = encode_parameter_state(version, &params, |id| seen[&id])
                .expect("finite values within a valid layout must encode");
            let decoded = decode_parameter_state(&bytes, version)
                .expect("a blob this build wrote must decode in this build");

            let expected: Vec<(u32, f64)> = seen.into_iter().collect();
            proptest::prop_assert_eq!(decoded.len(), expected.len());
            for (got, want) in decoded.iter().zip(expected.iter()) {
                proptest::prop_assert_eq!(got.0, want.0);
                // Bit equality, not `==`: it also rules out a -0.0/0.0 swap.
                proptest::prop_assert_eq!(got.1.to_bits(), want.1.to_bits());
            }
        }

        /// Order independence is what makes adding, removing and reordering
        /// fields in a `#[derive(Params)]` struct safe. Encoding the same
        /// values under a permuted layout must yield the same id-keyed map.
        #[test]
        fn the_decoded_map_does_not_depend_on_parameter_order(
            ids in proptest::collection::vec(proptest::prelude::any::<u32>(), 1..12),
            rotation in 0usize..12,
        ) {
            let mut unique: Vec<u32> = ids.clone();
            unique.sort_unstable();
            unique.dedup();
            let value_for = |id: u32| f64::from(id % 1_000) / 1_000.0;

            let forward: Vec<ParamInfo> =
                unique.iter().map(|id| ParamInfo::new(*id, "param")).collect();
            let mut rotated = unique.clone();
            let shift = rotation % rotated.len();
            rotated.rotate_left(shift);
            let rotated: Vec<ParamInfo> =
                rotated.iter().map(|id| ParamInfo::new(*id, "param")).collect();

            let a = decode_parameter_state(
                &encode_parameter_state(STATE_VERSION, &forward, value_for).unwrap(),
                STATE_VERSION,
            )
            .unwrap();
            let b = decode_parameter_state(
                &encode_parameter_state(STATE_VERSION, &rotated, value_for).unwrap(),
                STATE_VERSION,
            )
            .unwrap();

            let as_map = |entries: Vec<(u32, f64)>| {
                entries
                    .into_iter()
                    .map(|(id, value)| (id, value.to_bits()))
                    .collect::<std::collections::BTreeMap<_, _>>()
            };
            proptest::prop_assert_eq!(as_map(a), as_map(b));
        }

        /// A host can hand back a truncated file, a foreign file, or bytes from
        /// a crashed write. The decoder must answer `None` or a *complete*
        /// entry list — never panic, and never a partial list that the caller
        /// would apply as if it were a whole preset.
        #[test]
        fn decoding_arbitrary_bytes_never_panics_and_never_yields_a_partial_list(
            prefix_is_valid in proptest::prelude::any::<bool>(),
            version in 0u32..4,
            count in 0u32..600,
            body in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..800),
            current in 0u32..4,
        ) {
            let mut bytes = Vec::new();
            if prefix_is_valid {
                bytes.extend_from_slice(&STATE_MAGIC);
            } else {
                bytes.extend_from_slice(b"XXXXXXX\0");
            }
            bytes.extend_from_slice(&version.to_le_bytes());
            bytes.extend_from_slice(&count.to_le_bytes());
            bytes.extend_from_slice(&body);

            if let Some(entries) = decode_parameter_state(&bytes, current) {
                proptest::prop_assert_eq!(entries.len(), count as usize);
                proptest::prop_assert!(entries.iter().all(|(_, value)| value.is_finite()));
            }
        }

        /// The version rule is an inequality, not a match: this build reads
        /// anything it or an older build wrote, and refuses anything newer
        /// because it cannot know how a future layout reinterprets values.
        #[test]
        fn a_blob_is_readable_exactly_when_it_is_not_from_the_future(
            version in proptest::prelude::any::<u32>(),
            current in proptest::prelude::any::<u32>(),
        ) {
            let header = header_bytes(version, 0);
            proptest::prop_assert_eq!(
                decode_header(&header, current).is_some(),
                version <= current
            );
        }
    }
}
