use super::{is_vst3_module, load_plugin_library, HostPlugin, PluginFormat, PluginInfo};
use std::path::Path;

/// Scan a directory for audio plugins.
pub fn scan_directory(path: &Path) -> Vec<PluginInfo> {
    let mut results = Vec::new();
    if !path.exists() {
        return results;
    }

    // CLAP: .clap files
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if has_extension(&p, "clap") {
                results.extend(scan_clap(&p));
            }
        }
    }

    // VST3: .vst3 bundles (Contents/<arch>/<platform module>)
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if has_extension(&p, "vst3") {
                results.extend(scan_vst3(&p));
            }
        }
    }

    // AU: .component bundles (macOS only)
    #[cfg(all(target_os = "macos", feature = "au"))]
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if has_extension(&p, "component") {
                results.extend(scan_au(&p));
            }
        }
    }

    results
}

fn has_extension(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(expected))
}

/// Scan a .clap file for plugin descriptors.
pub fn scan_clap(path: &Path) -> Vec<PluginInfo> {
    let mut results = Vec::new();

    let dylib_path = match find_clap_module(path) {
        Some(p) => p,
        None => return results,
    };

    let lib = match unsafe { load_plugin_library(&dylib_path) } {
        Ok(lib) => lib,
        Err(e) => {
            eprintln!("  Failed to load {}: {}", dylib_path.display(), e);
            return results;
        }
    };

    unsafe {
        let entry: libloading::Symbol<*const clap_sys::entry::clap_plugin_entry_t> =
            match lib.get(b"clap_entry") {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("  No clap_entry symbol in {}: {}", dylib_path.display(), e);
                    return results;
                }
            };

        let entry = &**entry;

        let init = match entry.init {
            Some(f) => f,
            None => return results,
        };

        // For bundles, pass the bundle path; for plain dylibs, pass the dylib path
        let init_path = if path.is_dir() { path } else { &dylib_path };
        let path_c = std::ffi::CString::new(init_path.to_str().unwrap_or("")).unwrap();
        if !init(path_c.as_ptr()) {
            return results;
        }

        let get_factory = match entry.get_factory {
            Some(f) => f,
            None => {
                if let Some(deinit) = entry.deinit {
                    deinit();
                }
                return results;
            }
        };

        let factory_id = std::ffi::CString::new("clap.plugin-factory").unwrap();
        let factory_ptr = get_factory(factory_id.as_ptr());
        if factory_ptr.is_null() {
            if let Some(deinit) = entry.deinit {
                deinit();
            }
            return results;
        }

        let factory =
            &*(factory_ptr as *const clap_sys::factory::plugin_factory::clap_plugin_factory_t);
        let get_plugin_count = match factory.get_plugin_count {
            Some(f) => f,
            None => {
                if let Some(deinit) = entry.deinit {
                    deinit();
                }
                return results;
            }
        };

        let count = get_plugin_count(factory);
        let get_desc = match factory.get_plugin_descriptor {
            Some(f) => f,
            None => {
                if let Some(deinit) = entry.deinit {
                    deinit();
                }
                return results;
            }
        };

        for i in 0..count {
            let desc = get_desc(factory, i);
            if desc.is_null() {
                continue;
            }
            let desc = &*desc;
            results.push(PluginInfo {
                name: cstr(desc.name),
                vendor: cstr(desc.vendor),
                version: cstr(desc.version),
                id: cstr(desc.id),
                path: path.to_str().unwrap_or("").to_string(),
                format: PluginFormat::CLAP,
                class_index: 0,
                input_channels: 0,
                output_channels: 0,
                is_synth: false,
            });
        }

        if let Some(deinit) = entry.deinit {
            deinit();
        }
    }

    results
        .into_iter()
        .filter_map(
            |mut info| match super::clap_host::ClapHostPlugin::load(&info.path, &info.id) {
                Ok(mut plugin) => {
                    let runtime = plugin.info().clone();
                    info.input_channels = runtime.input_channels;
                    info.output_channels = runtime.output_channels;
                    info.is_synth = runtime.is_synth;
                    plugin.shutdown();
                    Some(info)
                }
                Err(error) => {
                    eprintln!("  Failed to probe CLAP plugin {}: {}", info.id, error);
                    None
                }
            },
        )
        .collect()
}

/// Scan a .vst3 bundle for plugin descriptors.
pub fn scan_vst3(path: &Path) -> Vec<PluginInfo> {
    let mut results = Vec::new();
    let dylib_path = find_vst3_module(path);
    let dylib_path = match dylib_path {
        Some(p) => p,
        None => return results,
    };

    let lib = match unsafe { load_plugin_library(&dylib_path) } {
        Ok(lib) => lib,
        Err(e) => {
            eprintln!("  Failed to load {}: {}", dylib_path.display(), e);
            return results;
        }
    };

    unsafe {
        let get_factory: libloading::Symbol<unsafe extern "C" fn() -> *mut std::ffi::c_void> =
            match lib.get(b"GetPluginFactory") {
                Ok(s) => s,
                Err(_) => return results,
            };

        let factory_ptr = get_factory();
        if factory_ptr.is_null() {
            return results;
        }

        // factory_ptr is a COM object pointer; first field is the vtable pointer
        let factory =
            &**(factory_ptr as *const *const vst3_sys::base::ipluginbase::IPluginFactoryVtbl);
        let mut info = vst3_sys::base::ipluginbase::PFactoryInfoData {
            vendor: [0; 64],
            url: [0; 256],
            email: [0; 128],
            flags: 0,
        };
        let _ = (factory.get_factory_info)(factory_ptr, &mut info);

        let count = (factory.count_classes)(factory_ptr);
        for i in 0..count {
            let mut class_info = vst3_sys::base::ipluginbase::PClassInfoData {
                cid: [0; 16],
                cardinality: 0,
                category: [0; 32],
                name: [0; 64],
            };
            if (factory.get_class_info)(factory_ptr, i, &mut class_info) == 0 {
                let category = cstr_from_char8(&class_info.category);
                if category == "Audio Module Class" {
                    results.push(PluginInfo {
                        name: cstr_from_char8(&class_info.name),
                        vendor: cstr_from_char8(&info.vendor),
                        version: String::new(),
                        id: format!("{:?}", class_info.cid),
                        path: path.to_str().unwrap_or("").to_string(),
                        format: PluginFormat::VST3,
                        class_index: i as u32,
                        input_channels: 0,
                        output_channels: 0,
                        is_synth: false,
                    });
                }
            }
        }

        // Release the factory
        let unknown = &**(factory_ptr as *const *const vst3_sys::base::types::IUnknownVtbl);
        (unknown.release)(factory_ptr);
    }

    results
        .into_iter()
        .filter_map(|mut info| {
            match super::vst3_host::Vst3HostPlugin::load(&info.path, info.class_index) {
                Ok(mut plugin) => {
                    let runtime = plugin.info().clone();
                    info.input_channels = runtime.input_channels;
                    info.output_channels = runtime.output_channels;
                    info.is_synth = runtime.is_synth;
                    plugin.shutdown();
                    Some(info)
                }
                Err(error) => {
                    eprintln!(
                        "  Failed to probe VST3 class {}: {}",
                        info.class_index, error
                    );
                    None
                }
            }
        })
        .collect()
}

/// Scan a .component bundle (AU) for plugin descriptors.
/// First tries the AudioComponent API (for installed components),
/// falls back to plist parsing for non-installed bundles.
#[cfg(all(target_os = "macos", feature = "au"))]
pub fn scan_au(path: &Path) -> Vec<PluginInfo> {
    let path_str = path.to_str().unwrap_or("");

    // Try AudioComponent API first - find components matching this bundle
    let plist_path = path.join("Contents").join("Info.plist");
    if let Ok(plist_data) = std::fs::read_to_string(&plist_path) {
        // Try to extract type/subtype/manufacturer from AudioComponents in plist
        if let Some((component_type, component_subtype, component_manufacturer)) =
            extract_au_description(&plist_data)
        {
            // Use direct lookup (works on macOS Sequoia where enumeration is broken)
            if let Some(component) = super::au_host::find_au_by_desc(
                component_type,
                component_subtype,
                component_manufacturer,
            ) {
                let name = super::au_host::au_component_name(component);
                let id = format!(
                    "{}-{}-{}",
                    fourcc_str(component_type),
                    fourcc_str(component_subtype),
                    fourcc_str(component_manufacturer)
                );
                return vec![PluginInfo {
                    name,
                    vendor: String::new(),
                    version: String::new(),
                    id,
                    path: path_str.to_string(),
                    format: PluginFormat::AU,
                    class_index: 0,
                    input_channels: 0,
                    output_channels: 0,
                    is_synth: false,
                }];
            }
        }
    }

    // Fallback: basic plist info
    if let Ok(plist_data) = std::fs::read_to_string(&plist_path) {
        if let Some(name) = extract_plist_string(&plist_data, "CFBundleName") {
            return vec![PluginInfo {
                name,
                vendor: String::new(),
                version: String::new(),
                id: path_str.to_string(),
                path: path_str.to_string(),
                format: PluginFormat::AU,
                class_index: 0,
                input_channels: 0,
                output_channels: 0,
                is_synth: false,
            }];
        }
    }

    Vec::new()
}

/// Scan all installed AU plugins on the system.
#[cfg(all(target_os = "macos", feature = "au"))]
pub fn scan_au_system() -> Vec<PluginInfo> {
    super::au_host::scan_au_components()
        .into_iter()
        .map(|(_component, desc, name)| PluginInfo {
            name,
            vendor: String::new(),
            version: String::new(),
            id: format!(
                "{}-{}-{}",
                fourcc_str(desc.componentType),
                fourcc_str(desc.componentSubType),
                fourcc_str(desc.componentManufacturer)
            ),
            path: String::new(),
            format: PluginFormat::AU,
            class_index: 0,
            input_channels: 0,
            output_channels: 0,
            is_synth: desc.componentType == 0x61756D75, // 'aumu'
        })
        .collect()
}

#[cfg(all(target_os = "macos", feature = "au"))]
fn fourcc_str(fourcc: u32) -> String {
    let bytes = [
        (fourcc >> 24) as u8,
        (fourcc >> 16) as u8,
        (fourcc >> 8) as u8,
        fourcc as u8,
    ];
    String::from_utf8_lossy(&bytes).to_string()
}

#[cfg(all(target_os = "macos", feature = "au"))]
fn find_au_component(
    component_type: u32,
    component_subtype: u32,
    component_manufacturer: u32,
) -> Option<PluginInfo> {
    let components = super::au_host::scan_au_components();
    for (_component, desc, name) in components {
        if desc.componentType == component_type
            && desc.componentSubType == component_subtype
            && desc.componentManufacturer == component_manufacturer
        {
            return Some(PluginInfo {
                name,
                vendor: String::new(),
                version: String::new(),
                id: format!(
                    "{}-{}-{}",
                    fourcc_str(desc.componentType),
                    fourcc_str(desc.componentSubType),
                    fourcc_str(desc.componentManufacturer)
                ),
                path: String::new(),
                format: PluginFormat::AU,
                class_index: 0,
                input_channels: 0,
                output_channels: 0,
                is_synth: desc.componentType == 0x61756D75,
            });
        }
    }
    None
}

/// Extract AU type/subtype/manufacturer from Info.plist AudioComponents.
#[cfg(all(target_os = "macos", feature = "au"))]
pub fn extract_au_description(plist: &str) -> Option<(u32, u32, u32)> {
    // Look for AudioComponents dict with type, subtype, manufacturer
    let type_str = extract_plist_value(plist, "type")?;
    let subtype_str = extract_plist_value(plist, "subtype")?;
    let manufacturer_str = extract_plist_value(plist, "manufacturer")?;

    let component_type = plist_fourcc(&type_str)?;
    let component_subtype = plist_fourcc(&subtype_str)?;
    let component_manufacturer = plist_fourcc(&manufacturer_str)?;

    Some((component_type, component_subtype, component_manufacturer))
}

/// Parse a plist fourcc string like "'aufx'" or "aufx" into OSType.
#[cfg(all(target_os = "macos", feature = "au"))]
pub fn plist_fourcc(s: &str) -> Option<u32> {
    let s = s.trim_matches('\'');
    if s.len() != 4 {
        return None;
    }
    let bytes = s.as_bytes();
    Some(
        (bytes[0] as u32) << 24
            | (bytes[1] as u32) << 16
            | (bytes[2] as u32) << 8
            | (bytes[3] as u32),
    )
}

/// Extract a plist value for a key (simple XML plist parser).
#[cfg(all(target_os = "macos", feature = "au"))]
fn extract_plist_value(plist: &str, key: &str) -> Option<String> {
    let key_tag = format!("<key>{}</key>", key);
    let pos = plist.find(&key_tag)?;
    let after = &plist[pos + key_tag.len()..];
    // Look for <string>...</string> or <integer>...</integer>
    if let Some(start) = after.find("<string>") {
        let after = &after[start + 8..];
        let end = after.find("</string>")?;
        return Some(after[..end].trim().to_string());
    }
    None
}

pub(super) fn find_clap_module(path: &Path) -> Option<std::path::PathBuf> {
    if !path.is_dir() {
        return Some(path.to_path_buf());
    }

    let module_dir = path.join("Contents").join("MacOS");
    let preferred_names = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| vec![stem.to_owned()])
        .unwrap_or_default();
    find_bundle_module(path, &module_dir, &preferred_names, is_clap_bundle_module)
}

pub(super) fn find_vst3_module(bundle: &Path) -> Option<std::path::PathBuf> {
    find_vst3_module_for_target(bundle, std::env::consts::OS, std::env::consts::ARCH)
}

fn find_vst3_module_for_target(
    bundle: &Path,
    target_os: &str,
    target_arch: &str,
) -> Option<std::path::PathBuf> {
    let directory = vst3_architecture_directory(target_os, target_arch)?;
    let module_dir = bundle.join("Contents").join(directory);
    let preferred_names = vst3_preferred_module_names(bundle, target_os);
    find_bundle_module(bundle, &module_dir, &preferred_names, is_vst3_module)
}

fn vst3_preferred_module_names(bundle: &Path, target_os: &str) -> Vec<String> {
    let Some(stem) = bundle.file_stem().and_then(|stem| stem.to_str()) else {
        return Vec::new();
    };
    match target_os {
        "macos" => vec![stem.to_owned()],
        "windows" => vec![format!("{stem}.vst3"), format!("{stem}.dll")],
        "linux" => vec![format!("{stem}.so")],
        _ => Vec::new(),
    }
}

fn vst3_architecture_directory(target_os: &str, target_arch: &str) -> Option<&'static str> {
    match (target_os, target_arch) {
        ("macos", _) => Some("MacOS"),
        ("windows", "x86") => Some("x86-win"),
        ("windows", "x86_64") => Some("x86_64-win"),
        ("windows", "aarch64") => Some("arm_64-win"),
        ("linux", "x86") => Some("i386-linux"),
        ("linux", "x86_64") => Some("x86_64-linux"),
        ("linux", "aarch64") => Some("aarch64-linux"),
        ("linux", "riscv64") => Some("riscv64-linux"),
        _ => None,
    }
}

fn find_bundle_module(
    bundle: &Path,
    module_dir: &Path,
    preferred_names: &[String],
    is_module: fn(&Path) -> bool,
) -> Option<std::path::PathBuf> {
    if !module_dir.is_dir() {
        return None;
    }

    let plist_path = bundle.join("Contents").join("Info.plist");
    let executable = std::fs::read_to_string(plist_path)
        .ok()
        .and_then(|plist| extract_plist_string(&plist, "CFBundleExecutable"));
    for name in executable
        .into_iter()
        .chain(preferred_names.iter().cloned())
    {
        if !is_single_path_component(&name) {
            continue;
        }
        let candidate = module_dir.join(name);
        if candidate.is_file() && is_module(&candidate) {
            return Some(candidate);
        }
    }

    let mut candidates: Vec<_> = std::fs::read_dir(module_dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_module(path))
        .collect();
    candidates.sort();
    if candidates.len() == 1 {
        candidates.pop()
    } else {
        None
    }
}

fn is_single_path_component(value: &str) -> bool {
    let mut components = Path::new(value).components();
    matches!(
        (components.next(), components.next()),
        (Some(std::path::Component::Normal(_)), None)
    )
}

fn is_clap_bundle_module(path: &Path) -> bool {
    match path.extension().and_then(|extension| extension.to_str()) {
        None => path.file_name().is_some(),
        Some(extension) => ["clap", "dylib", "so", "dll"]
            .iter()
            .any(|expected| extension.eq_ignore_ascii_case(expected)),
    }
}

fn cstr(ptr: *const std::ffi::c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_str()
        .unwrap_or("")
        .to_string()
}

fn cstr_from_char8(buf: &[std::os::raw::c_char]) -> String {
    let bytes: Vec<u8> = buf.iter().map(|&b| b as u8).collect();
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).to_string()
}

fn extract_plist_string(plist: &str, key: &str) -> Option<String> {
    let key_tag = format!("<key>{}</key>", key);
    let pos = plist.find(&key_tag)?;
    let after = &plist[pos + key_tag.len()..];
    let string_start = after.find("<string>")?;
    let after = &after[string_start + 8..];
    let end = after.find("</string>")?;
    Some(xml_unescape(after[..end].trim()))
}

/// Decode the XML entities emitted by the packager's plist writer.
///
/// This intentionally handles only the five predefined XML entities. Numeric
/// entities are not needed for bundle metadata and leaving unknown entities
/// untouched is safer than silently changing a plugin name.
fn xml_unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        // Decode ampersands last so an input such as `&amp;lt;` is decoded once
        // to the literal text `&lt;`, rather than twice to `<`.
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let id = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "sunmao-runner-module-discovery-{}-{id}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn join(&self, path: &str) -> PathBuf {
            self.0.join(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn top_level_plugin_extensions_are_case_insensitive() {
        assert!(has_extension(Path::new("Plugin.CLAP"), "clap"));
        assert!(has_extension(Path::new("Plugin.VST3"), "vst3"));
        assert!(has_extension(Path::new("Plugin.Component"), "component"));
        assert!(!has_extension(Path::new("Plugin.txt"), "vst3"));
    }

    #[test]
    fn vst3_architecture_directories_match_packager_layouts() {
        assert_eq!(
            vst3_architecture_directory("windows", "x86_64"),
            Some("x86_64-win")
        );
        assert_eq!(
            vst3_architecture_directory("windows", "aarch64"),
            Some("arm_64-win")
        );
        assert_eq!(
            vst3_architecture_directory("linux", "aarch64"),
            Some("aarch64-linux")
        );
        assert_eq!(
            vst3_architecture_directory("linux", "riscv64"),
            Some("riscv64-linux")
        );
        assert_eq!(
            vst3_architecture_directory("macos", "aarch64"),
            Some("MacOS")
        );
        assert_eq!(vst3_architecture_directory("windows", "mips64"), None);
    }

    #[test]
    fn vst3_discovery_uses_the_requested_target_architecture() {
        let temp = TempDir::new();
        let bundle = temp.join("Targeted.vst3");
        let arm_module = bundle.join("Contents/arm_64-win/Targeted.vst3");
        let x64_module = bundle.join("Contents/x86_64-win/Targeted.vst3");
        std::fs::create_dir_all(arm_module.parent().unwrap()).unwrap();
        std::fs::create_dir_all(x64_module.parent().unwrap()).unwrap();
        std::fs::write(&arm_module, b"arm64").unwrap();
        std::fs::write(&x64_module, b"x64").unwrap();

        assert_eq!(
            find_vst3_module_for_target(&bundle, "windows", "aarch64"),
            Some(arm_module)
        );
        assert_eq!(
            find_vst3_module_for_target(&bundle, "windows", "x86_64"),
            Some(x64_module)
        );
    }

    #[test]
    fn vst3_discovery_rejects_ambiguous_fallbacks_but_prefers_bundle_stem() {
        let temp = TempDir::new();
        let bundle = temp.join("Preferred.vst3");
        let module_dir = bundle.join("Contents/x86_64-win");
        std::fs::create_dir_all(&module_dir).unwrap();
        std::fs::write(module_dir.join("First.vst3"), b"first").unwrap();
        std::fs::write(module_dir.join("Second.dll"), b"second").unwrap();

        assert_eq!(
            find_vst3_module_for_target(&bundle, "windows", "x86_64"),
            None
        );

        let preferred = module_dir.join("Preferred.vst3");
        std::fs::write(&preferred, b"preferred").unwrap();
        assert_eq!(
            find_vst3_module_for_target(&bundle, "windows", "x86_64"),
            Some(preferred)
        );
    }

    #[test]
    fn bundle_discovery_accepts_extensionless_plist_executables() {
        let temp = TempDir::new();

        for extension in ["vst3", "clap"] {
            let bundle = temp.join(&format!("Bundle.{extension}"));
            let module = bundle.join("Contents/MacOS/ActualExecutable");
            std::fs::create_dir_all(module.parent().unwrap()).unwrap();
            std::fs::write(&module, b"module").unwrap();
            std::fs::write(
                bundle.join("Contents/Info.plist"),
                "<key>CFBundleExecutable</key><string>ActualExecutable</string>",
            )
            .unwrap();

            let discovered = if extension == "vst3" {
                find_vst3_module_for_target(&bundle, "macos", "aarch64")
            } else {
                find_clap_module(&bundle)
            };
            assert_eq!(discovered, Some(module));
        }
    }

    #[test]
    fn bundle_discovery_unescapes_plist_executable_names() {
        let temp = TempDir::new();
        let bundle = temp.join("Bundle & Name.vst3");
        let module = bundle.join("Contents/MacOS/Bundle & Name");
        std::fs::create_dir_all(module.parent().unwrap()).unwrap();
        std::fs::write(&module, b"module").unwrap();
        std::fs::write(
            bundle.join("Contents/Info.plist"),
            "<key>CFBundleExecutable</key><string>Bundle &amp; Name</string>",
        )
        .unwrap();

        assert_eq!(
            find_vst3_module_for_target(&bundle, "macos", "aarch64"),
            Some(module)
        );
    }

    #[test]
    fn plist_entity_decoding_happens_once() {
        assert_eq!(xml_unescape("A &amp; B &lt; C"), "A & B < C");
        assert_eq!(xml_unescape("&amp;lt;"), "&lt;");
    }
}
