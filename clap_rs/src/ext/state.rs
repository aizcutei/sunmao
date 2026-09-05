//! CLAP state extension.

use crate::ext::params::ParameterInfo;
use crate::plugin::Plugin;
use crate::plugin_instance::{ffi_guard, instance_ptr};
use clap_sys::ext::state::clap_plugin_state_t;
use clap_sys::plugin::clap_plugin_t;
use clap_sys::stream::{clap_istream_t, clap_ostream_t};

const STATE_MAGIC: [u8; 8] = *b"SMCLPRM\0";
/// Version the encoder tests round-trip against; live blobs carry the plugin's
/// own `Plugin::STATE_VERSION`.
#[cfg(test)]
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

unsafe fn save_parameter_state<P: Plugin>(
    plugin: &P,
    params: &[ParameterInfo],
    stream: *const clap_ostream_t,
) -> bool {
    let Some(bytes) =
        encode_parameter_state(P::STATE_VERSION, params, |id| plugin.get_parameter(id))
    else {
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
    let Some((version, count)) = decode_header(&header, P::STATE_VERSION) else {
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

    let Some(entries) = decode_parameter_state(&bytes, P::STATE_VERSION) else {
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
    // Only after every value has been applied: the plugin migrates from a
    // complete older state, never a half-applied one.
    if version < P::STATE_VERSION {
        plugin.state_loaded(version);
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
    version: u32,
    params: &[ParameterInfo],
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
    use std::ffi::c_void;

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

        let reordered = [parameter(42, "Mode"), parameter(7, "Gain")];
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
        let header = header_bytes(STATE_VERSION.saturating_sub(1), 0);
        assert_eq!(
            decode_header(&header, STATE_VERSION),
            Some((STATE_VERSION.saturating_sub(1), 0))
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

    /// A plugin that records what a state load did to it, so a property can
    /// assert on the *effect* of `load_parameter_state` rather than only on
    /// the codec underneath it.
    struct StateProbe {
        params: Vec<ParameterInfo>,
        applied: Vec<(u32, f64)>,
        migrated_from: Option<u32>,
    }

    impl crate::plugin::Plugin for StateProbe {
        type AudioProcessor = ();

        const STATE_VERSION: u32 = 3;

        fn new(_host: crate::plugin::HostHandle) -> Self {
            unreachable!("the probe is constructed directly by the tests")
        }

        fn activate(
            &mut self,
            _sample_rate: f64,
            _min_frames: u32,
            _max_frames: u32,
        ) -> Option<Self::AudioProcessor> {
            Some(())
        }

        fn declare_parameters(&self) -> Vec<ParameterInfo> {
            self.params.clone()
        }

        fn get_parameter(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_parameter(&mut self, id: u32, value: f64) {
            self.applied.push((id, value));
        }

        fn state_loaded(&mut self, from_version: u32) {
            self.migrated_from = Some(from_version);
        }
    }

    /// A `clap_istream_t` over a byte slice, matching how a host feeds a
    /// preset back: short reads are legal, so the reader must loop.
    struct ByteStream {
        bytes: Vec<u8>,
        offset: usize,
    }

    unsafe extern "C" fn byte_stream_read(
        stream: *const clap_istream_t,
        buffer: *mut c_void,
        size: u64,
    ) -> i64 {
        let state = unsafe { &mut *((*stream).ctx as *mut ByteStream) };
        let remaining = state.bytes.len() - state.offset;
        let count = remaining.min(size as usize);
        if count > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    state.bytes[state.offset..].as_ptr(),
                    buffer.cast::<u8>(),
                    count,
                );
            }
            state.offset += count;
        }
        count as i64
    }

    fn load_from_bytes(plugin: &mut StateProbe, bytes: Vec<u8>) -> bool {
        let params = plugin.params.clone();
        let mut state = ByteStream { bytes, offset: 0 };
        let stream = clap_istream_t {
            ctx: (&raw mut state).cast(),
            read: Some(byte_stream_read),
        };
        unsafe { load_parameter_state(plugin, &params, &stream) }
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
                (proptest::prelude::any::<u32>(), 0u32..5, 0.0f64..1.0),
                0..24,
            ),
            version in 0u32..STATE_VERSION + 1,
        ) {
            // CLAP stores plain values, so each parameter's value has to sit
            // inside the range that parameter declares.
            let mut seen = std::collections::BTreeMap::new();
            for (id, steps, fraction) in &entries {
                let max = f64::from((*steps).max(1));
                seen.insert(*id, (max, fraction * max));
            }
            let params: Vec<ParameterInfo> = seen
                .iter()
                .map(|(id, (max, _))| ParameterInfo {
                    id: *id,
                    name: "param".to_string(),
                    module: String::new(),
                    min_value: 0.0,
                    max_value: *max,
                    default_value: 0.0,
                    is_stepped: false,
                })
                .collect();

            let bytes = encode_parameter_state(version, &params, |id| seen[&id].1)
                .expect("in-range values must encode");
            let decoded = decode_parameter_state(&bytes, STATE_VERSION)
                .expect("a blob this build wrote must decode in this build");

            proptest::prop_assert_eq!(decoded.len(), seen.len());
            for (got, (id, (_, value))) in decoded.iter().zip(seen.iter()) {
                proptest::prop_assert_eq!(got.0, *id);
                // Bit equality, not `==`: it also rules out a -0.0/0.0 swap.
                proptest::prop_assert_eq!(got.1.to_bits(), value.to_bits());
            }
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

        /// The load path's contract, end to end: a rejected blob leaves the
        /// plugin untouched (no value applied, no migration), and an accepted
        /// one applies every known id before migrating — never the reverse
        /// order, which would show the plugin a half-restored preset.
        #[test]
        fn a_rejected_state_never_reaches_the_plugin(
            version in 0u32..6,
            declared in proptest::collection::vec(0u32..8, 0..6),
            written in proptest::collection::vec((0u32..8, -1.0f64..3.0), 0..6),
            truncate in 0usize..4,
        ) {
            let params: Vec<ParameterInfo> = {
                let mut ids = declared.clone();
                ids.sort_unstable();
                ids.dedup();
                ids.iter()
                    .map(|id| ParameterInfo {
                        id: *id,
                        name: "param".to_string(),
                        module: String::new(),
                        min_value: 0.0,
                        max_value: 1.0,
                        default_value: 0.0,
                        is_stepped: false,
                    })
                    .collect()
            };

            // Hand-build the blob so out-of-range values and truncation —
            // which the encoder refuses to produce — still get exercised.
            let mut blob = header_bytes(version, written.len() as u32);
            for (id, value) in &written {
                blob.extend_from_slice(&id.to_le_bytes());
                blob.extend_from_slice(&value.to_le_bytes());
            }
            blob.truncate(blob.len().saturating_sub(truncate));

            let mut plugin = StateProbe {
                params,
                applied: Vec::new(),
                migrated_from: None,
            };
            let accepted = load_from_bytes(&mut plugin, blob);

            let known: Vec<(u32, f64)> = written
                .iter()
                .copied()
                .filter(|(id, _)| plugin.params.iter().any(|param| param.id == *id))
                .collect();
            let all_in_range = known.iter().all(|(_, value)| (0.0..=1.0).contains(value));
            let complete = truncate == 0;

            proptest::prop_assert_eq!(
                accepted,
                complete && version <= StateProbe::STATE_VERSION && all_in_range
            );
            if accepted {
                proptest::prop_assert_eq!(&plugin.applied, &known);
                proptest::prop_assert_eq!(
                    plugin.migrated_from,
                    (version < StateProbe::STATE_VERSION).then_some(version)
                );
            } else {
                proptest::prop_assert!(plugin.applied.is_empty());
                proptest::prop_assert_eq!(plugin.migrated_from, None);
            }
        }
    }
}
