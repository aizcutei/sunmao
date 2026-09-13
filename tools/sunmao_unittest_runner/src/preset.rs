//! Format-native preset containers, written and read from the host side.
//!
//! A preset is not the same thing as a state blob. State is whatever bytes the
//! plugin hands the host through its own API; a preset is a *file* that other
//! software is expected to read. The two formats answer that very differently,
//! and the difference is the whole reason this module exists rather than the
//! host just writing `save_state()` to disk under a different extension.
//!
//! * **VST3** specifies a container. `.vstpreset` carries the plugin's class ID
//!   and a chunk list, so a host can refuse a preset that belongs to a
//!   different plugin before handing any bytes to it.
//! * **CLAP** specifies no such file. `clap.preset-discovery` lets a plugin
//!   declare where its presets live and what they look like
//!   (`CLAP_PRESET_DISCOVERY_LOCATION_FILE`), which means the file format is
//!   the plugin's to choose. The host-side equivalent of "save a preset" is
//!   therefore the `clap.state` stream verbatim.
//!
//! The layouts below are transcribed from the upstream sources, not recalled:
//! `vstpresetfile.h`/`.cpp` for the container and `funknown.cpp` for the class
//! ID string.

/// `kHeader` chunk ID, `vstpresetfile.cpp`.
pub const VST3_HEADER_ID: [u8; 4] = *b"VST3";
/// `kComponentState` chunk ID.
pub const VST3_COMPONENT_STATE_ID: [u8; 4] = *b"Comp";
/// `kControllerState` chunk ID.
pub const VST3_CONTROLLER_STATE_ID: [u8; 4] = *b"Cont";
/// `kChunkList` chunk ID.
pub const VST3_CHUNK_LIST_ID: [u8; 4] = *b"List";
/// `kFormatVersion`, `vstpresetfile.cpp`.
pub const VST3_FORMAT_VERSION: i32 = 1;
/// `kClassIDSize`: the class ID is stored as 32 ASCII hex characters.
pub const VST3_CLASS_ID_SIZE: usize = 32;
/// Header is ID(4) + version(4) + class string(32) + list offset(8).
pub const VST3_HEADER_SIZE: usize = 4 + 4 + VST3_CLASS_ID_SIZE + 8;

/// Whether this platform's VST3 host reads a `TUID`'s first eight bytes as a
/// COM `GUID` struct.
///
/// `fplatform.h` sets `COM_COMPATIBLE 1` under `defined (_WIN32)` and `0` for
/// `__gnu_linux__`/`__linux__` and `__APPLE__`. The runner is emulating a host
/// on the platform it is running on, so it must match that platform's answer.
pub const fn host_uses_com_uid_layout() -> bool {
    cfg!(target_os = "windows")
}

/// Render a `TUID`'s raw bytes the way `FUID::toString` does.
///
/// Upstream (`funknown.cpp`) has two branches. Without `COM_COMPATIBLE` it is
/// `toString8 (string, data, 0, 16)` — all sixteen bytes in order as uppercase
/// hex. With it, the first eight bytes are reinterpreted as a `GuidStruct`
/// (`uint32 Data1; uint16 Data2; uint16 Data3;`) and printed as
/// `"%08X%04X%04X%s"`, with the remaining eight bytes appended in order. On a
/// little-endian machine that reverses the first four bytes, then each of the
/// next two pairs.
///
/// **The two branches disagree for the same bytes**, which matters here: a
/// plugin whose class ID is a fixed byte array rather than an `INLINE_UID`
/// gets a different preset class string per platform. See
/// `docs/phase2/semantics.md` and the tests at the bottom of this file.
pub fn fuid_to_string(class_id: &[i8; 16], com_compatible: bool) -> String {
    let bytes: [u8; 16] = std::array::from_fn(|index| class_id[index] as u8);
    let mut out = String::with_capacity(VST3_CLASS_ID_SIZE);
    if com_compatible {
        let data1 = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let data2 = u16::from_le_bytes([bytes[4], bytes[5]]);
        let data3 = u16::from_le_bytes([bytes[6], bytes[7]]);
        out.push_str(&format!("{data1:08X}{data2:04X}{data3:04X}"));
        for byte in &bytes[8..] {
            out.push_str(&format!("{byte:02X}"));
        }
    } else {
        for byte in &bytes {
            out.push_str(&format!("{byte:02X}"));
        }
    }
    debug_assert_eq!(out.len(), VST3_CLASS_ID_SIZE);
    out
}

/// A `.vstpreset` as read back from disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vst3Preset {
    /// The 32-character class string exactly as stored, not re-derived.
    pub class_string: String,
    pub component_state: Vec<u8>,
    /// `Cont`. SunMao is a single-component design and does not write one, but
    /// presets from other vendors carry it and dropping it silently would make
    /// a round-trip through this host lossy.
    pub controller_state: Option<Vec<u8>>,
}

/// Build a `.vstpreset` holding one component-state chunk.
///
/// Layout, per `PresetFile::writeHeader` and `PresetFile::writeChunkList`:
/// `'VST3'`, `int32` version, 32 ASCII class characters, `int64` offset of the
/// chunk list; then the chunk payloads; then `'List'`, `int32` entry count, and
/// one `{ id(4), int64 offset, int64 size }` per entry. Every integer is
/// little-endian — upstream only byte-swaps under `BYTEORDER == kBigEndian`.
pub fn write_vst3_preset(class_id: &[i8; 16], component_state: &[u8]) -> Vec<u8> {
    let class_string = fuid_to_string(class_id, host_uses_com_uid_layout());
    write_vst3_preset_with_class_string(&class_string, component_state)
}

/// Same as [`write_vst3_preset`] but with the class string supplied directly,
/// so tests can pin both UID layouts without pretending to be another platform.
pub fn write_vst3_preset_with_class_string(class_string: &str, component_state: &[u8]) -> Vec<u8> {
    assert_eq!(
        class_string.len(),
        VST3_CLASS_ID_SIZE,
        "a VST3 preset class string is exactly {VST3_CLASS_ID_SIZE} ASCII characters"
    );

    let list_offset = VST3_HEADER_SIZE + component_state.len();
    let mut bytes = Vec::with_capacity(list_offset + 4 + 4 + 20);

    bytes.extend_from_slice(&VST3_HEADER_ID);
    bytes.extend_from_slice(&VST3_FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(class_string.as_bytes());
    bytes.extend_from_slice(&(list_offset as i64).to_le_bytes());

    bytes.extend_from_slice(component_state);

    bytes.extend_from_slice(&VST3_CHUNK_LIST_ID);
    bytes.extend_from_slice(&1i32.to_le_bytes());
    bytes.extend_from_slice(&VST3_COMPONENT_STATE_ID);
    bytes.extend_from_slice(&(VST3_HEADER_SIZE as i64).to_le_bytes());
    bytes.extend_from_slice(&(component_state.len() as i64).to_le_bytes());

    bytes
}

fn read_i32_le(bytes: &[u8], at: usize) -> Result<i32, String> {
    bytes
        .get(at..at + 4)
        .map(|slice| i32::from_le_bytes(slice.try_into().expect("4 bytes")))
        .ok_or_else(|| format!("preset truncated: wanted 4 bytes at offset {at}"))
}

fn read_i64_le(bytes: &[u8], at: usize) -> Result<i64, String> {
    bytes
        .get(at..at + 8)
        .map(|slice| i64::from_le_bytes(slice.try_into().expect("8 bytes")))
        .ok_or_else(|| format!("preset truncated: wanted 8 bytes at offset {at}"))
}

fn chunk_slice(bytes: &[u8], offset: i64, size: i64, what: &str) -> Result<Vec<u8>, String> {
    if offset < 0 || size < 0 {
        return Err(format!("{what} chunk has a negative offset or size"));
    }
    let start = offset as usize;
    let end = start
        .checked_add(size as usize)
        .ok_or_else(|| format!("{what} chunk offset and size overflow"))?;
    bytes
        .get(start..end)
        .map(<[u8]>::to_vec)
        .ok_or_else(|| format!("{what} chunk runs past the end of the file"))
}

/// Parse a `.vstpreset`.
///
/// Rejection is the point of this function, not a side effect: a preset file
/// arrives from disk and may be truncated, from another plugin, or not a
/// preset at all.
pub fn read_vst3_preset(bytes: &[u8]) -> Result<Vst3Preset, String> {
    if bytes.len() < VST3_HEADER_SIZE {
        return Err(format!(
            "not a VST3 preset: {} bytes is shorter than the {VST3_HEADER_SIZE}-byte header",
            bytes.len()
        ));
    }
    if bytes[0..4] != VST3_HEADER_ID {
        return Err("not a VST3 preset: missing the 'VST3' header ID".into());
    }
    let version = read_i32_le(bytes, 4)?;
    if version != VST3_FORMAT_VERSION {
        return Err(format!(
            "unsupported VST3 preset format version {version}, expected {VST3_FORMAT_VERSION}"
        ));
    }
    let class_string = std::str::from_utf8(&bytes[8..8 + VST3_CLASS_ID_SIZE])
        .map_err(|_| "VST3 preset class ID is not ASCII".to_string())?
        .to_string();
    if !class_string.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("VST3 preset class ID is not 32 hexadecimal characters".into());
    }

    let list_offset = read_i64_le(bytes, 40)?;
    if list_offset <= 0 {
        return Err(format!(
            "VST3 preset chunk list offset {list_offset} is not positive"
        ));
    }
    let list_offset = list_offset as usize;
    if bytes.get(list_offset..list_offset + 4) != Some(&VST3_CHUNK_LIST_ID[..]) {
        return Err("VST3 preset chunk list is missing its 'List' ID".into());
    }
    let entry_count = read_i32_le(bytes, list_offset + 4)?;
    if entry_count < 0 {
        return Err(format!("VST3 preset declares {entry_count} chunk entries"));
    }

    let mut component_state = None;
    let mut controller_state = None;
    for entry in 0..entry_count as usize {
        let at = list_offset + 8 + entry * 20;
        let id = bytes
            .get(at..at + 4)
            .ok_or_else(|| format!("VST3 preset chunk entry {entry} is truncated"))?;
        let offset = read_i64_le(bytes, at + 4)?;
        let size = read_i64_le(bytes, at + 12)?;
        if id == VST3_COMPONENT_STATE_ID {
            component_state = Some(chunk_slice(bytes, offset, size, "component state")?);
        } else if id == VST3_CONTROLLER_STATE_ID {
            controller_state = Some(chunk_slice(bytes, offset, size, "controller state")?);
        }
    }

    Ok(Vst3Preset {
        class_string,
        component_state: component_state
            .ok_or_else(|| "VST3 preset has no 'Comp' component-state chunk".to_string())?,
        controller_state,
    })
}

/// Bytes to write for a CLAP preset: the `clap.state` stream verbatim.
///
/// CLAP has no host-defined preset container. Wrapping the stream in one that
/// SunMao invented would produce a file no other CLAP host could read, which is
/// the opposite of what a preset is for.
pub fn clap_preset_bytes(state: &[u8]) -> &[u8] {
    state
}

/// Validate a file before handing it to `clap.state`'s `load`.
///
/// There is no container to check, so the only thing the host can usefully
/// catch is a file that plainly belongs to another format — feeding a
/// `.vstpreset` to `clap.state` would otherwise reach the plugin's decoder and
/// come back as an opaque "load failed".
pub fn read_clap_preset(bytes: &[u8]) -> Result<&[u8], String> {
    if bytes.is_empty() {
        return Err("CLAP preset file is empty".into());
    }
    if bytes.len() >= 4 && bytes[0..4] == VST3_HEADER_ID {
        return Err(
            "this is a VST3 preset, not CLAP state: it begins with the 'VST3' header ID".into(),
        );
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The IID of `IComponent`, whose bytes under each layout are already
    /// pinned by `vst3_sys`'s own `vst3_uid_layout_matches_native_and_com_golden_bytes`.
    /// Starting from those same bytes means this test and that one cannot drift
    /// apart silently.
    const ICOMPONENT_NATIVE: [i8; 16] = [
        0xE8u8 as i8,
        0x31,
        0xFFu8 as i8,
        0x31,
        0xF2u8 as i8,
        0xD5u8 as i8,
        0x43,
        0x01,
        0x92u8 as i8,
        0x8Eu8 as i8,
        0xBBu8 as i8,
        0xEEu8 as i8,
        0x25,
        0x69,
        0x78,
        0x02,
    ];

    #[test]
    fn a_native_layout_uid_prints_its_bytes_in_order() {
        assert_eq!(
            fuid_to_string(&ICOMPONENT_NATIVE, false),
            "E831FF31F2D54301928EBBEE25697802"
        );
    }

    #[test]
    fn the_com_layout_reverses_the_first_three_guid_fields() {
        // Same sixteen bytes, read as `uint32 Data1; uint16 Data2; uint16 Data3`
        // on a little-endian machine: E8,31,FF,31 -> 31FF31E8, and so on.
        assert_eq!(
            fuid_to_string(&ICOMPONENT_NATIVE, true),
            "31FF31E8D5F20143928EBBEE25697802"
        );
    }

    /// The real property, checked against `vst3_sys` rather than against a
    /// string typed out by hand: whichever layout a platform stores a UID in,
    /// `FUID::toString` of those bytes is the *same* canonical text. That is
    /// exactly what `INLINE_UID` exists to guarantee, and it is what makes the
    /// asymmetry in the next test a real finding rather than an arithmetic slip.
    #[test]
    fn both_uid_layouts_print_the_same_canonical_string() {
        use vst3_sys::base::types::make_tuid_for_layout;

        for (l1, l2, l3, l4) in [
            (
                0xE831_FF31u32,
                0xF2D5_4301u32,
                0x928E_BBEEu32,
                0x2569_7802u32,
            ),
            (0xDCD7_BBE3, 0x7742_448D, 0xA874_AACC, 0x979C_759E),
            (0x0000_0001, 0x0002_0003, 0x0004_0005, 0x0006_0007),
        ] {
            let canonical = format!("{l1:08X}{l2:08X}{l3:08X}{l4:08X}");
            assert_eq!(
                fuid_to_string(&make_tuid_for_layout(l1, l2, l3, l4, false), false),
                canonical
            );
            assert_eq!(
                fuid_to_string(&make_tuid_for_layout(l1, l2, l3, l4, true), true),
                canonical
            );
        }
    }

    /// This is the finding, stated as a test rather than as prose: a class ID
    /// that is a fixed byte array — which is what `class_id_from_str` produces —
    /// does not survive the platform change that `INLINE_UID` is designed to
    /// absorb.
    #[test]
    fn the_same_bytes_yield_different_class_strings_on_windows_and_elsewhere() {
        let native = fuid_to_string(&ICOMPONENT_NATIVE, false);
        let com = fuid_to_string(&ICOMPONENT_NATIVE, true);
        assert_ne!(
            native, com,
            "if these ever agree the asymmetry documented in semantics.md is gone"
        );
        assert_eq!(native.len(), VST3_CLASS_ID_SIZE);
        assert_eq!(com.len(), VST3_CLASS_ID_SIZE);
        // The tail past the GUID struct is shared; only the first eight bytes move.
        assert_eq!(native[16..], com[16..]);
    }

    #[test]
    fn a_preset_round_trips_through_the_container() {
        let state = b"SMV3PRM\0\x01\x00\x00\x00 arbitrary payload".to_vec();
        let bytes = write_vst3_preset(&ICOMPONENT_NATIVE, &state);
        let preset = read_vst3_preset(&bytes).expect("round trip");
        assert_eq!(preset.component_state, state);
        assert_eq!(
            preset.class_string,
            fuid_to_string(&ICOMPONENT_NATIVE, host_uses_com_uid_layout())
        );
        assert_eq!(preset.controller_state, None);
    }

    #[test]
    fn an_empty_state_still_produces_a_readable_preset() {
        let bytes = write_vst3_preset(&ICOMPONENT_NATIVE, &[]);
        let preset = read_vst3_preset(&bytes).expect("round trip");
        assert!(preset.component_state.is_empty());
    }

    #[test]
    fn the_header_lands_where_the_layout_says_it_does() {
        let bytes = write_vst3_preset(&ICOMPONENT_NATIVE, b"payload");
        assert_eq!(&bytes[0..4], b"VST3");
        assert_eq!(i32::from_le_bytes(bytes[4..8].try_into().unwrap()), 1);
        assert_eq!(bytes[8..40].len(), VST3_CLASS_ID_SIZE);
        let list_offset = i64::from_le_bytes(bytes[40..48].try_into().unwrap()) as usize;
        assert_eq!(list_offset, VST3_HEADER_SIZE + b"payload".len());
        assert_eq!(&bytes[list_offset..list_offset + 4], b"List");
        assert_eq!(&bytes[VST3_HEADER_SIZE..list_offset], b"payload");
    }

    #[test]
    fn a_controller_chunk_written_by_another_vendor_is_preserved() {
        // Hand-assemble a two-entry preset, since SunMao never writes 'Cont'.
        let class_string = fuid_to_string(&ICOMPONENT_NATIVE, false);
        let comp = b"component".to_vec();
        let cont = b"controller".to_vec();
        let list_offset = VST3_HEADER_SIZE + comp.len() + cont.len();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&VST3_HEADER_ID);
        bytes.extend_from_slice(&VST3_FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(class_string.as_bytes());
        bytes.extend_from_slice(&(list_offset as i64).to_le_bytes());
        bytes.extend_from_slice(&comp);
        bytes.extend_from_slice(&cont);
        bytes.extend_from_slice(&VST3_CHUNK_LIST_ID);
        bytes.extend_from_slice(&2i32.to_le_bytes());
        bytes.extend_from_slice(&VST3_COMPONENT_STATE_ID);
        bytes.extend_from_slice(&(VST3_HEADER_SIZE as i64).to_le_bytes());
        bytes.extend_from_slice(&(comp.len() as i64).to_le_bytes());
        bytes.extend_from_slice(&VST3_CONTROLLER_STATE_ID);
        bytes.extend_from_slice(&((VST3_HEADER_SIZE + comp.len()) as i64).to_le_bytes());
        bytes.extend_from_slice(&(cont.len() as i64).to_le_bytes());

        let preset = read_vst3_preset(&bytes).expect("two-entry preset");
        assert_eq!(preset.component_state, comp);
        assert_eq!(preset.controller_state, Some(cont));
    }

    #[test]
    fn a_file_that_is_not_a_preset_is_rejected() {
        let error = read_vst3_preset(b"this is not a preset at all, not even close!!!!!!!!")
            .expect_err("must reject");
        assert!(error.contains("'VST3' header ID"), "{error}");
    }

    #[test]
    fn a_truncated_preset_is_rejected_rather_than_indexed_past_the_end() {
        let bytes = write_vst3_preset(&ICOMPONENT_NATIVE, b"payload");
        for cut in 0..bytes.len() {
            // The only requirement is that it never panics and never claims success.
            let _ = read_vst3_preset(&bytes[..cut]);
        }
        assert!(read_vst3_preset(&bytes[..VST3_HEADER_SIZE]).is_err());
    }

    #[test]
    fn a_chunk_pointing_past_the_end_is_rejected() {
        let mut bytes = write_vst3_preset(&ICOMPONENT_NATIVE, b"payload");
        let list_offset = i64::from_le_bytes(bytes[40..48].try_into().unwrap()) as usize;
        let size_at = list_offset + 8 + 12;
        bytes[size_at..size_at + 8].copy_from_slice(&i64::MAX.to_le_bytes());
        let error = read_vst3_preset(&bytes).expect_err("must reject");
        assert!(
            error.contains("past the end") || error.contains("overflow"),
            "{error}"
        );
    }

    #[test]
    fn a_preset_with_no_component_chunk_is_rejected() {
        let class_string = fuid_to_string(&ICOMPONENT_NATIVE, false);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&VST3_HEADER_ID);
        bytes.extend_from_slice(&VST3_FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(class_string.as_bytes());
        bytes.extend_from_slice(&(VST3_HEADER_SIZE as i64).to_le_bytes());
        bytes.extend_from_slice(&VST3_CHUNK_LIST_ID);
        bytes.extend_from_slice(&0i32.to_le_bytes());
        let error = read_vst3_preset(&bytes).expect_err("must reject");
        assert!(error.contains("'Comp'"), "{error}");
    }

    #[test]
    fn a_future_format_version_is_rejected() {
        let mut bytes = write_vst3_preset(&ICOMPONENT_NATIVE, b"payload");
        bytes[4..8].copy_from_slice(&2i32.to_le_bytes());
        let error = read_vst3_preset(&bytes).expect_err("must reject");
        assert!(error.contains("version 2"), "{error}");
    }

    #[test]
    fn clap_presets_are_the_state_stream_verbatim() {
        let state = b"SMCLPRM\0\x01\x00\x00\x00payload";
        assert_eq!(clap_preset_bytes(state), state);
        assert_eq!(read_clap_preset(state).expect("accepted"), state);
    }

    #[test]
    fn a_vst3_preset_handed_to_the_clap_loader_is_named_as_such() {
        let bytes = write_vst3_preset(&ICOMPONENT_NATIVE, b"payload");
        let error = read_clap_preset(&bytes).expect_err("must reject");
        assert!(error.contains("VST3 preset"), "{error}");
    }

    #[test]
    fn an_empty_clap_preset_is_rejected() {
        assert!(read_clap_preset(&[]).is_err());
    }
}
