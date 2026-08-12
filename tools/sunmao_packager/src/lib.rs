use anyhow::{bail, Context, Result};
use std::fs::{self, File};
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PackageFormat {
    Au,
    Vst3,
    Clap,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TargetPlatform {
    Macos,
    Windows,
    Linux,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Target {
    pub platform: TargetPlatform,
    pub architecture: String,
}

impl Target {
    pub fn new(platform: TargetPlatform, architecture: impl Into<String>) -> Self {
        Self {
            platform,
            architecture: architecture.into(),
        }
    }

    pub fn current() -> Result<Self> {
        #[cfg(target_os = "macos")]
        let platform = TargetPlatform::Macos;
        #[cfg(target_os = "windows")]
        let platform = TargetPlatform::Windows;
        #[cfg(target_os = "linux")]
        let platform = TargetPlatform::Linux;
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        bail!("SunMao packaging is unsupported on this operating system");

        Ok(Self::new(platform, std::env::consts::ARCH))
    }
}

#[derive(Clone, Debug)]
pub struct AuMetadata {
    pub component_type: String,
    pub component_subtype: String,
    pub manufacturer: String,
    pub factory: String,
    pub sandbox_safe: bool,
}

#[derive(Clone, Debug)]
pub struct PackageRequest {
    pub format: PackageFormat,
    pub binary: PathBuf,
    pub out: PathBuf,
    pub name: String,
    pub bundle_id: String,
    pub version: String,
    pub codesign: bool,
    pub au: Option<AuMetadata>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum OutputKind {
    Bundle,
    File,
}

struct ValidatedRequest<'a> {
    request: &'a PackageRequest,
    target: &'a Target,
    output: PathBuf,
    module_stem: String,
    version: [u32; 3],
}

pub fn package(request: &PackageRequest) -> Result<PathBuf> {
    package_for_target(request, &Target::current()?)
}

/// Packages for an explicit target. This is useful to validate cross-platform
/// layouts in build tooling; the command-line interface always uses the current
/// compilation target.
pub fn package_for_target(request: &PackageRequest, target: &Target) -> Result<PathBuf> {
    let validated = validate_request(request, target)?;
    let output = validated.output.clone();

    stage_and_publish(&output, |staging| {
        match request.format {
            PackageFormat::Au => build_au(&validated, staging)?,
            PackageFormat::Vst3 => build_vst3(&validated, staging)?,
            PackageFormat::Clap => build_clap(&validated, staging)?,
        }

        if request.codesign {
            codesign_bundle(staging)?;
        }

        Ok(())
    })
    .with_context(|| format!("failed to package {}", output.display()))?;

    Ok(output)
}

fn validate_request<'a>(
    request: &'a PackageRequest,
    target: &'a Target,
) -> Result<ValidatedRequest<'a>> {
    validate_display_name(&request.name)?;
    validate_bundle_id(&request.bundle_id)?;
    let version = parse_version(&request.version)?;

    if request.format == PackageFormat::Au {
        if target.platform != TargetPlatform::Macos {
            bail!("AudioUnit packaging is only supported on macOS");
        }

        let au = request
            .au
            .as_ref()
            .context("AudioUnit metadata is required for AU format")?;
        validate_fourcc("AU component type", &au.component_type)?;
        validate_fourcc("AU component subtype", &au.component_subtype)?;
        validate_fourcc("AU manufacturer", &au.manufacturer)?;
        validate_factory_name(&au.factory)?;
        version_to_au_integer(version)?;
    }

    if request.codesign {
        if target.platform != TargetPlatform::Macos {
            bail!("--codesign is only supported for macOS bundles");
        }
        if Target::current()?.platform != TargetPlatform::Macos {
            bail!("macOS bundles can only be codesigned on macOS");
        }
    }

    validate_binary(&request.binary, target)?;

    if request.out.as_os_str().is_empty() || request.out.file_name().is_none() {
        bail!("output path must name a plugin or bundle");
    }

    let extension = match request.format {
        PackageFormat::Au => "component",
        PackageFormat::Vst3 => "vst3",
        PackageFormat::Clap => "clap",
    };
    let output = request.out.with_extension(extension);
    let module_stem = output
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.trim().is_empty() && *stem != "." && *stem != "..")
        .context("output path must have a valid UTF-8 plugin name")?
        .to_owned();

    let kind = output_kind(request.format, target.platform);
    validate_output_path(&request.binary, &output, kind)?;

    if request.format == PackageFormat::Vst3 && target.platform != TargetPlatform::Macos {
        vst3_architecture_directory(target)?;
    }

    Ok(ValidatedRequest {
        request,
        target,
        output,
        module_stem,
        version,
    })
}

fn validate_display_name(name: &str) -> Result<()> {
    validate_required_text("plugin name", name, 255)
}

fn validate_required_text(label: &str, value: &str, max_len: usize) -> Result<()> {
    if value.is_empty() || value.trim() != value {
        bail!("{label} must be non-empty and have no leading or trailing whitespace");
    }
    if value.len() > max_len {
        bail!("{label} is longer than {max_len} bytes");
    }
    if value.chars().any(char::is_control) {
        bail!("{label} must not contain control characters");
    }
    Ok(())
}

fn validate_bundle_id(bundle_id: &str) -> Result<()> {
    validate_required_text("bundle identifier", bundle_id, 255)?;
    let segments: Vec<_> = bundle_id.split('.').collect();
    if segments.len() < 2 {
        bail!("bundle identifier must contain at least two dot-separated components");
    }

    for segment in segments {
        if segment.is_empty()
            || !segment
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || !segment
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            || !segment
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_alphanumeric)
        {
            bail!(
                "bundle identifier components must start and end with an ASCII letter or digit and contain only letters, digits, and '-'"
            );
        }
    }

    Ok(())
}

fn parse_version(version: &str) -> Result<[u32; 3]> {
    validate_required_text("version", version, 64)?;
    let parts: Vec<_> = version.split('.').collect();
    if parts.is_empty() || parts.len() > 3 {
        bail!("version must contain one to three numeric components");
    }

    let mut parsed = [0; 3];
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            bail!("version must contain only dot-separated decimal integers");
        }
        parsed[index] = part
            .parse()
            .with_context(|| format!("version component '{part}' is too large"))?;
    }
    Ok(parsed)
}

fn version_to_au_integer(version: [u32; 3]) -> Result<u32> {
    let [major, minor, patch] = version;
    if major > u16::MAX as u32 || minor > u8::MAX as u32 || patch > u8::MAX as u32 {
        bail!("AU version components must fit 16.8.8 bits (major <= 65535, minor/patch <= 255)");
    }
    Ok((major << 16) | (minor << 8) | patch)
}

fn validate_fourcc(label: &str, value: &str) -> Result<()> {
    if value.len() != 4 || !value.bytes().all(|byte| byte.is_ascii_graphic()) {
        bail!("{label} must be exactly four printable ASCII characters");
    }
    Ok(())
}

fn validate_factory_name(factory: &str) -> Result<()> {
    validate_required_text("AU factory function", factory, 255)?;
    let mut bytes = factory.bytes();
    let first = bytes.next().context("AU factory function is required")?;
    if !(first.is_ascii_alphabetic() || first == b'_')
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        bail!("AU factory function must be a valid C identifier");
    }
    Ok(())
}

fn validate_output_path(binary: &Path, output: &Path, kind: OutputKind) -> Result<()> {
    let parent = output_parent(output);
    validate_existing_ancestor(parent)?;

    match fs::symlink_metadata(output) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                bail!(
                    "output path must not be a symbolic link: {}",
                    output.display()
                );
            }
            match kind {
                OutputKind::Bundle if !metadata.is_dir() => {
                    bail!(
                        "bundle output already exists and is not a directory: {}",
                        output.display()
                    )
                }
                OutputKind::File if !metadata.is_file() => {
                    bail!(
                        "plugin output already exists and is not a regular file: {}",
                        output.display()
                    )
                }
                _ => {}
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("failed to inspect output path"),
    }

    let binary = fs::canonicalize(binary).context("failed to canonicalize input binary")?;
    if let Ok(output) = fs::canonicalize(output) {
        if binary == output {
            bail!("input binary and output path must be different");
        }
    } else if let (Ok(parent), Some(name)) = (fs::canonicalize(parent), output.file_name()) {
        if binary == parent.join(name) {
            bail!("input binary and output path must be different");
        }
    }

    Ok(())
}

fn validate_existing_ancestor(path: &Path) -> Result<()> {
    let mut candidate = path;
    loop {
        match fs::metadata(candidate) {
            Ok(metadata) => {
                if !metadata.is_dir() {
                    bail!("output parent is not a directory: {}", candidate.display());
                }
                return Ok(());
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                candidate = candidate.parent().unwrap_or_else(|| Path::new("."));
            }
            Err(error) => return Err(error).context("failed to inspect output parent"),
        }
    }
}

fn output_kind(format: PackageFormat, platform: TargetPlatform) -> OutputKind {
    match (format, platform) {
        (PackageFormat::Clap, TargetPlatform::Windows | TargetPlatform::Linux) => OutputKind::File,
        _ => OutputKind::Bundle,
    }
}

fn validate_binary(binary: &Path, target: &Target) -> Result<()> {
    let metadata = fs::metadata(binary)
        .with_context(|| format!("input binary does not exist: {}", binary.display()))?;
    if !metadata.is_file() {
        bail!("input binary is not a regular file: {}", binary.display());
    }

    let expected_extension = match target.platform {
        TargetPlatform::Macos => "dylib",
        TargetPlatform::Windows => "dll",
        TargetPlatform::Linux => "so",
    };
    let actual_extension = binary
        .extension()
        .and_then(|extension| extension.to_str())
        .context("input binary must have a UTF-8 file extension")?;
    if !actual_extension.eq_ignore_ascii_case(expected_extension) {
        bail!(
            "input binary for {:?} must have a .{expected_extension} extension, got {}",
            target.platform,
            binary.display()
        );
    }

    let mut file = File::open(binary).context("failed to open input binary")?;
    match target.platform {
        TargetPlatform::Macos => validate_macho(&mut file, &target.architecture),
        TargetPlatform::Windows => validate_pe_dll(&mut file, &target.architecture),
        TargetPlatform::Linux => validate_elf_shared_object(&mut file, &target.architecture),
    }
    .with_context(|| format!("invalid plugin module {}", binary.display()))
}

#[derive(Copy, Clone)]
enum Endian {
    Little,
    Big,
}

fn read_u16(bytes: &[u8], endian: Endian) -> u16 {
    match endian {
        Endian::Little => u16::from_le_bytes([bytes[0], bytes[1]]),
        Endian::Big => u16::from_be_bytes([bytes[0], bytes[1]]),
    }
}

fn read_u32(bytes: &[u8], endian: Endian) -> u32 {
    match endian {
        Endian::Little => u32::from_le_bytes(bytes.try_into().expect("four bytes")),
        Endian::Big => u32::from_be_bytes(bytes.try_into().expect("four bytes")),
    }
}

fn read_u64(bytes: &[u8], endian: Endian) -> u64 {
    match endian {
        Endian::Little => u64::from_le_bytes(bytes.try_into().expect("eight bytes")),
        Endian::Big => u64::from_be_bytes(bytes.try_into().expect("eight bytes")),
    }
}

fn validate_elf_shared_object(file: &mut File, architecture: &str) -> Result<()> {
    let (expected_class, expected_machine, expected_encoding) = match architecture {
        "x86" => (1, 3, 1),
        "x86_64" => (2, 62, 1),
        "arm" => (1, 40, 1),
        "aarch64" => (2, 183, 1),
        "riscv64" => (2, 243, 1),
        other => bail!("unsupported Linux plugin architecture '{other}'"),
    };

    let mut ident = [0; 16];
    file.read_exact(&mut ident)
        .context("ELF header is truncated")?;
    if &ident[..4] != b"\x7fELF" {
        bail!("expected an ELF shared object");
    }
    let class = match ident[4] {
        1 => 1,
        2 => 2,
        _ => bail!("ELF file has an invalid class"),
    };
    if class != expected_class {
        bail!("ELF class does not match target architecture '{architecture}'");
    }
    let endian = match ident[5] {
        1 => Endian::Little,
        2 => Endian::Big,
        _ => bail!("ELF file has an invalid byte order"),
    };
    if ident[5] != expected_encoding {
        bail!("ELF byte order does not match target architecture '{architecture}'");
    }

    let header_len = if class == 1 { 52 } else { 64 };
    let mut header = vec![0; header_len];
    header[..ident.len()].copy_from_slice(&ident);
    file.read_exact(&mut header[ident.len()..])
        .context("ELF header is truncated")?;
    if read_u16(&header[16..18], endian) != 3 {
        bail!("ELF file is not a shared object (ET_DYN)");
    }
    let machine = read_u16(&header[18..20], endian);
    if machine != expected_machine {
        bail!("ELF architecture does not match target architecture '{architecture}'");
    }

    let (program_header_offset, program_header_entry_size, program_header_count) = if class == 1 {
        (
            read_u32(&header[28..32], endian) as u64,
            read_u16(&header[42..44], endian) as u64,
            read_u16(&header[44..46], endian) as u64,
        )
    } else {
        (
            read_u64(&header[32..40], endian),
            read_u16(&header[54..56], endian) as u64,
            read_u16(&header[56..58], endian) as u64,
        )
    };

    if program_header_count > 0 {
        let minimum_entry_size = if class == 1 { 32 } else { 56 };
        if program_header_offset == 0 || program_header_entry_size < minimum_entry_size {
            bail!("ELF program header table is invalid");
        }

        // ET_DYN is shared by libraries and PIE executables. PT_INTERP is an
        // executable-only marker, so reject it without guessing about cases
        // where the ELF metadata cannot safely distinguish the two.
        for index in 0..program_header_count {
            let entry_offset = index
                .checked_mul(program_header_entry_size)
                .and_then(|offset| program_header_offset.checked_add(offset))
                .context("ELF program header table offset overflows")?;
            file.seek(SeekFrom::Start(entry_offset))
                .context("failed to seek to ELF program header")?;
            let mut program_type = [0; 4];
            file.read_exact(&mut program_type)
                .context("ELF program header table is truncated")?;
            if read_u32(&program_type, endian) == 3 {
                bail!("ELF module is a PIE executable (PT_INTERP), not a shared object");
            }
        }
    }
    Ok(())
}

fn validate_pe_dll(file: &mut File, architecture: &str) -> Result<()> {
    let mut dos_header = [0; 64];
    file.read_exact(&mut dos_header)
        .context("PE DOS header is truncated")?;
    if &dos_header[..2] != b"MZ" {
        bail!("expected a Windows PE/COFF module");
    }
    let pe_offset = u32::from_le_bytes(dos_header[60..64].try_into().unwrap()) as u64;
    let mut coff = [0; 24];
    file.seek(SeekFrom::Start(pe_offset))
        .context("failed to seek to PE header")?;
    file.read_exact(&mut coff)
        .context("PE header is truncated")?;
    if &coff[..4] != b"PE\0\0" {
        bail!("PE signature is missing");
    }

    let expected_machine = match architecture {
        "x86" => 0x014c,
        "x86_64" => 0x8664,
        "aarch64" => 0xaa64,
        other => bail!("unsupported Windows plugin architecture '{other}'"),
    };
    let machine = u16::from_le_bytes(coff[4..6].try_into().unwrap());
    if machine != expected_machine {
        bail!("PE architecture does not match target architecture '{architecture}'");
    }
    let characteristics = u16::from_le_bytes(coff[22..24].try_into().unwrap());
    if characteristics & 0x2000 == 0 {
        bail!("PE module is not marked as a DLL");
    }
    Ok(())
}

fn validate_macho(file: &mut File, architecture: &str) -> Result<()> {
    let mut prefix = [0; 16];
    file.read_exact(&mut prefix)
        .context("Mach-O header is truncated")?;

    match &prefix[..4] {
        [0xce, 0xfa, 0xed, 0xfe] | [0xcf, 0xfa, 0xed, 0xfe] => {
            validate_thin_macho_header(&prefix, Endian::Little, architecture)
        }
        [0xfe, 0xed, 0xfa, 0xce] | [0xfe, 0xed, 0xfa, 0xcf] => {
            validate_thin_macho_header(&prefix, Endian::Big, architecture)
        }
        [0xca, 0xfe, 0xba, 0xbe] => {
            validate_fat_macho(file, &prefix, Endian::Big, false, architecture)
        }
        [0xbe, 0xba, 0xfe, 0xca] => {
            validate_fat_macho(file, &prefix, Endian::Little, false, architecture)
        }
        [0xca, 0xfe, 0xba, 0xbf] => {
            validate_fat_macho(file, &prefix, Endian::Big, true, architecture)
        }
        [0xbf, 0xba, 0xfe, 0xca] => {
            validate_fat_macho(file, &prefix, Endian::Little, true, architecture)
        }
        _ => bail!("expected a Mach-O dynamic library or bundle"),
    }
}

fn macho_cpu_type(architecture: &str) -> Result<u32> {
    match architecture {
        "x86" => Ok(7),
        "x86_64" => Ok(0x0100_0007),
        "arm" => Ok(12),
        "aarch64" => Ok(0x0100_000c),
        other => bail!("unsupported macOS plugin architecture '{other}'"),
    }
}

fn validate_thin_macho_header(header: &[u8; 16], endian: Endian, architecture: &str) -> Result<()> {
    if read_u32(&header[4..8], endian) != macho_cpu_type(architecture)? {
        bail!("Mach-O architecture does not match target architecture '{architecture}'");
    }
    match read_u32(&header[12..16], endian) {
        6 | 8 => Ok(()),
        _ => bail!("Mach-O file is not a dynamic library or bundle"),
    }
}

fn validate_fat_macho(
    file: &mut File,
    prefix: &[u8; 16],
    endian: Endian,
    is_64_bit: bool,
    architecture: &str,
) -> Result<()> {
    let slice_count = read_u32(&prefix[4..8], endian);
    if slice_count == 0 || slice_count > 64 {
        bail!("Mach-O universal binary has an invalid slice count");
    }
    let expected_cpu = macho_cpu_type(architecture)?;
    let entry_size = if is_64_bit { 32 } else { 20 };

    for index in 0..slice_count as u64 {
        let entry_offset = 8 + index * entry_size;
        let mut entry = [0; 32];
        file.seek(SeekFrom::Start(entry_offset))?;
        file.read_exact(&mut entry[..entry_size as usize])
            .context("Mach-O universal architecture table is truncated")?;
        if read_u32(&entry[..4], endian) != expected_cpu {
            continue;
        }

        let slice_offset = if is_64_bit {
            read_u64(&entry[8..16], endian)
        } else {
            read_u32(&entry[8..12], endian) as u64
        };
        let mut header = [0; 16];
        file.seek(SeekFrom::Start(slice_offset))?;
        file.read_exact(&mut header)
            .context("Mach-O universal slice header is truncated")?;
        return match &header[..4] {
            [0xce, 0xfa, 0xed, 0xfe] | [0xcf, 0xfa, 0xed, 0xfe] => {
                validate_thin_macho_header(&header, Endian::Little, architecture)
            }
            [0xfe, 0xed, 0xfa, 0xce] | [0xfe, 0xed, 0xfa, 0xcf] => {
                validate_thin_macho_header(&header, Endian::Big, architecture)
            }
            _ => bail!("Mach-O universal slice has an invalid header"),
        };
    }

    bail!("Mach-O universal binary does not contain target architecture '{architecture}'")
}

fn build_au(validated: &ValidatedRequest<'_>, staging: &Path) -> Result<()> {
    let au = validated
        .request
        .au
        .as_ref()
        .expect("AU metadata was validated");
    let contents = staging.join("Contents");
    let module = contents.join("MacOS").join(&validated.module_stem);
    fs::create_dir_all(module.parent().unwrap()).context("failed to create AU module directory")?;
    fs::create_dir_all(contents.join("Resources")).context("failed to create AU resources")?;
    fs::copy(&validated.request.binary, &module).context("failed to copy AU module")?;

    let plist = au_plist(validated, au)?;
    fs::write(contents.join("Info.plist"), plist).context("failed to write AU Info.plist")?;
    fs::write(contents.join("PkgInfo"), "BNDL????").context("failed to write AU PkgInfo")?;
    Ok(())
}

fn build_vst3(validated: &ValidatedRequest<'_>, staging: &Path) -> Result<()> {
    let contents = staging.join("Contents");
    match validated.target.platform {
        TargetPlatform::Macos => {
            let module = contents.join("MacOS").join(&validated.module_stem);
            fs::create_dir_all(module.parent().unwrap())
                .context("failed to create VST3 module directory")?;
            fs::create_dir_all(contents.join("Resources"))
                .context("failed to create VST3 resources")?;
            fs::copy(&validated.request.binary, module).context("failed to copy VST3 module")?;
            fs::write(contents.join("Info.plist"), bundle_plist(validated))
                .context("failed to write VST3 Info.plist")?;
            fs::write(contents.join("PkgInfo"), "BNDL????")
                .context("failed to write VST3 PkgInfo")?;
        }
        TargetPlatform::Windows => {
            let module = contents
                .join(vst3_architecture_directory(validated.target)?)
                .join(format!("{}.vst3", validated.module_stem));
            fs::create_dir_all(module.parent().unwrap())
                .context("failed to create VST3 architecture directory")?;
            fs::copy(&validated.request.binary, module).context("failed to copy VST3 module")?;
        }
        TargetPlatform::Linux => {
            let module = contents
                .join(vst3_architecture_directory(validated.target)?)
                .join(format!("{}.so", validated.module_stem));
            fs::create_dir_all(module.parent().unwrap())
                .context("failed to create VST3 architecture directory")?;
            fs::copy(&validated.request.binary, module).context("failed to copy VST3 module")?;
        }
    }
    Ok(())
}

fn build_clap(validated: &ValidatedRequest<'_>, staging: &Path) -> Result<()> {
    match validated.target.platform {
        TargetPlatform::Macos => {
            let contents = staging.join("Contents");
            let module = contents.join("MacOS").join(&validated.module_stem);
            fs::create_dir_all(module.parent().unwrap())
                .context("failed to create CLAP module directory")?;
            fs::create_dir_all(contents.join("Resources"))
                .context("failed to create CLAP resources")?;
            fs::copy(&validated.request.binary, module).context("failed to copy CLAP module")?;
            fs::write(contents.join("Info.plist"), bundle_plist(validated))
                .context("failed to write CLAP Info.plist")?;
            fs::write(contents.join("PkgInfo"), "BNDL????")
                .context("failed to write CLAP PkgInfo")?;
        }
        TargetPlatform::Windows | TargetPlatform::Linux => {
            fs::copy(&validated.request.binary, staging).context("failed to copy CLAP module")?;
        }
    }
    Ok(())
}

fn vst3_architecture_directory(target: &Target) -> Result<&'static str> {
    match (target.platform, target.architecture.as_str()) {
        (TargetPlatform::Windows, "x86") => Ok("x86-win"),
        (TargetPlatform::Windows, "x86_64") => Ok("x86_64-win"),
        (TargetPlatform::Windows, "aarch64") => Ok("arm_64-win"),
        (TargetPlatform::Linux, "x86") => Ok("i386-linux"),
        (TargetPlatform::Linux, "x86_64") => Ok("x86_64-linux"),
        (TargetPlatform::Linux, "aarch64") => Ok("aarch64-linux"),
        (TargetPlatform::Linux, "riscv64") => Ok("riscv64-linux"),
        (TargetPlatform::Macos, _) => {
            bail!("macOS VST3 bundles do not use architecture directories")
        }
        (platform, architecture) => {
            bail!("unsupported VST3 architecture '{architecture}' for {platform:?}")
        }
    }
}

fn au_plist(validated: &ValidatedRequest<'_>, au: &AuMetadata) -> Result<String> {
    let version_int = version_to_au_integer(validated.version)?;
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>English</string>
    <key>CFBundleExecutable</key>
    <string>{exec_name}</string>
    <key>CFBundleIdentifier</key>
    <string>{bundle_id}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>{name}</string>
    <key>CFBundlePackageType</key>
    <string>BNDL</string>
    <key>CFBundleSignature</key>
    <string>????</string>
    <key>CFBundleVersion</key>
    <string>{version}</string>
    <key>CFBundleShortVersionString</key>
    <string>{version}</string>
    <key>AudioComponents</key>
    <array>
        <dict>
            <key>description</key>
            <string>{name}</string>
            <key>factoryFunction</key>
            <string>{factory}</string>
            <key>manufacturer</key>
            <string>{manufacturer}</string>
            <key>name</key>
            <string>{name}</string>
            <key>sandboxSafe</key>
            <{sandbox}/>
            <key>subtype</key>
            <string>{subtype}</string>
            <key>type</key>
            <string>{component_type}</string>
            <key>version</key>
            <integer>{version_int}</integer>
        </dict>
    </array>
</dict>
</plist>
"#,
        exec_name = xml_escape(&validated.module_stem),
        bundle_id = xml_escape(&validated.request.bundle_id),
        name = xml_escape(&validated.request.name),
        version = xml_escape(&validated.request.version),
        factory = xml_escape(&au.factory),
        manufacturer = xml_escape(&au.manufacturer),
        sandbox = if au.sandbox_safe { "true" } else { "false" },
        subtype = xml_escape(&au.component_subtype),
        component_type = xml_escape(&au.component_type),
    ))
}

fn bundle_plist(validated: &ValidatedRequest<'_>) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>English</string>
    <key>CFBundleExecutable</key>
    <string>{exec_name}</string>
    <key>CFBundleIdentifier</key>
    <string>{bundle_id}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>{name}</string>
    <key>CFBundlePackageType</key>
    <string>BNDL</string>
    <key>CFBundleSignature</key>
    <string>????</string>
    <key>CFBundleVersion</key>
    <string>{version}</string>
    <key>CFBundleShortVersionString</key>
    <string>{version}</string>
</dict>
</plist>
"#,
        exec_name = xml_escape(&validated.module_stem),
        bundle_id = xml_escape(&validated.request.bundle_id),
        name = xml_escape(&validated.request.name),
        version = xml_escape(&validated.request.version),
    )
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

static UNIQUE_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

fn stage_and_publish(output: &Path, build: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
    let parent = output_parent(output);
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create output directory {}", parent.display()))?;
    let staging = unique_sibling(output, "stage")?;

    if let Err(error) = build(&staging) {
        let _ = remove_path(&staging);
        return Err(error);
    }

    if let Err(error) = publish_staging(&staging, output) {
        let _ = remove_path(&staging);
        return Err(error);
    }
    Ok(())
}

fn publish_staging(staging: &Path, output: &Path) -> Result<()> {
    if !output.exists() {
        return fs::rename(staging, output).context("failed to publish staged plugin");
    }

    let backup = unique_sibling(output, "backup")?;
    fs::rename(output, &backup).context("failed to move previous output aside")?;
    if let Err(publish_error) = fs::rename(staging, output) {
        if let Err(rollback_error) = fs::rename(&backup, output) {
            bail!(
                "failed to publish staged plugin ({publish_error}); also failed to restore previous output ({rollback_error})"
            );
        }
        return Err(publish_error)
            .context("failed to publish staged plugin; previous output restored");
    }

    remove_path(&backup)
        .context("plugin was published but the previous output backup could not be removed")
}

fn unique_sibling(output: &Path, purpose: &str) -> Result<PathBuf> {
    let parent = output_parent(output);
    let file_name = output
        .file_name()
        .and_then(|name| name.to_str())
        .context("output filename must be valid UTF-8")?;
    for _ in 0..1024 {
        let sequence = UNIQUE_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{file_name}.sunmao-{purpose}-{}-{sequence}",
            std::process::id()
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    bail!("could not allocate a unique staging path")
}

fn output_parent(output: &Path) -> &Path {
    output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn remove_path(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(path).context("failed to remove directory")
        }
        Ok(_) => fs::remove_file(path).context("failed to remove file"),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("failed to inspect path for removal"),
    }
}

#[cfg(target_os = "macos")]
fn codesign_bundle(bundle_path: &Path) -> Result<()> {
    let status = Command::new("codesign")
        .args(["--force", "--sign", "-", "--deep"])
        .arg(bundle_path)
        .status()
        .context("failed to execute codesign")?;
    if !status.success() {
        bail!("codesign failed with status: {status}");
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn codesign_bundle(_bundle_path: &Path) -> Result<()> {
    bail!("codesign is only available on macOS")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "sunmao-packager-unit-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn request(format: PackageFormat, binary: PathBuf, out: PathBuf) -> PackageRequest {
        PackageRequest {
            format,
            binary,
            out,
            name: "Test & Plugin".into(),
            bundle_id: "com.sunmao.test-plugin".into(),
            version: "1.2.3".into(),
            codesign: false,
            au: None,
        }
    }

    fn write_elf_with_class_and_encoding(path: &Path, class: u8, machine: u16, encoding: u8) {
        let header_len = if class == 1 { 52 } else { 64 };
        let mut bytes = vec![0; header_len];
        bytes[..4].copy_from_slice(b"\x7fELF");
        bytes[4] = class;
        bytes[5] = encoding;
        let file_type = if encoding == 2 {
            3u16.to_be_bytes()
        } else {
            3u16.to_le_bytes()
        };
        let machine = if encoding == 2 {
            machine.to_be_bytes()
        } else {
            machine.to_le_bytes()
        };
        bytes[16..18].copy_from_slice(&file_type);
        bytes[18..20].copy_from_slice(&machine);
        fs::write(path, bytes).unwrap();
    }

    fn write_elf_with_class(path: &Path, class: u8, machine: u16) {
        write_elf_with_class_and_encoding(path, class, machine, 1);
    }

    fn write_elf(path: &Path, machine: u16) {
        write_elf_with_class(path, 2, machine);
    }

    fn write_elf_pie(path: &Path, machine: u16) {
        let mut bytes = vec![0; 64 + 56];
        bytes[..4].copy_from_slice(b"\x7fELF");
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[16..18].copy_from_slice(&3u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&machine.to_le_bytes());
        bytes[32..40].copy_from_slice(&64u64.to_le_bytes());
        bytes[54..56].copy_from_slice(&56u16.to_le_bytes());
        bytes[56..58].copy_from_slice(&1u16.to_le_bytes());
        bytes[64..68].copy_from_slice(&3u32.to_le_bytes());
        fs::write(path, bytes).unwrap();
    }

    fn write_pe(path: &Path, machine: u16) {
        let mut bytes = vec![0; 128];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
        bytes[64..68].copy_from_slice(b"PE\0\0");
        bytes[68..70].copy_from_slice(&machine.to_le_bytes());
        bytes[86..88].copy_from_slice(&0x2000u16.to_le_bytes());
        fs::write(path, bytes).unwrap();
    }

    fn write_macho(path: &Path, cpu_type: u32) {
        let mut bytes = vec![0; 32];
        bytes[..4].copy_from_slice(&[0xcf, 0xfa, 0xed, 0xfe]);
        bytes[4..8].copy_from_slice(&cpu_type.to_le_bytes());
        bytes[12..16].copy_from_slice(&6u32.to_le_bytes());
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn creates_standard_vst3_layouts_and_module_names() {
        let temp = TempDir::new();

        let mac_binary = temp.path().join("libsource.dylib");
        write_macho(&mac_binary, 0x0100_000c);
        let mac = request(
            PackageFormat::Vst3,
            mac_binary,
            temp.path().join("Mac Product"),
        );
        let mac_output =
            package_for_target(&mac, &Target::new(TargetPlatform::Macos, "aarch64")).unwrap();
        assert!(mac_output.join("Contents/MacOS/Mac Product").is_file());
        let plist = fs::read_to_string(mac_output.join("Contents/Info.plist")).unwrap();
        assert!(plist.contains("<string>Mac Product</string>"));
        assert!(plist.contains("Test &amp; Plugin"));

        let win_binary = temp.path().join("source.dll");
        write_pe(&win_binary, 0x8664);
        let win = request(
            PackageFormat::Vst3,
            win_binary,
            temp.path().join("WinProduct.vst3"),
        );
        let win_output =
            package_for_target(&win, &Target::new(TargetPlatform::Windows, "x86_64")).unwrap();
        assert!(win_output
            .join("Contents/x86_64-win/WinProduct.vst3")
            .is_file());

        let linux_binary = temp.path().join("libsource.so");
        write_elf(&linux_binary, 183);
        let linux = request(
            PackageFormat::Vst3,
            linux_binary,
            temp.path().join("LinuxProduct"),
        );
        let linux_output =
            package_for_target(&linux, &Target::new(TargetPlatform::Linux, "aarch64")).unwrap();
        assert!(linux_output
            .join("Contents/aarch64-linux/LinuxProduct.so")
            .is_file());
    }

    #[test]
    fn creates_clap_bundle_on_macos_and_single_files_elsewhere() {
        let temp = TempDir::new();

        let mac_binary = temp.path().join("source.dylib");
        write_macho(&mac_binary, 0x0100_000c);
        let mac = request(PackageFormat::Clap, mac_binary, temp.path().join("MacClap"));
        let mac_output =
            package_for_target(&mac, &Target::new(TargetPlatform::Macos, "aarch64")).unwrap();
        assert!(mac_output.join("Contents/MacOS/MacClap").is_file());
        assert!(!mac_output.join("Contents/_CodeSignature").exists());

        let linux_binary = temp.path().join("source.so");
        write_elf(&linux_binary, 62);
        let linux = request(
            PackageFormat::Clap,
            linux_binary,
            temp.path().join("LinuxClap"),
        );
        let linux_output =
            package_for_target(&linux, &Target::new(TargetPlatform::Linux, "x86_64")).unwrap();
        assert_eq!(linux_output.extension().unwrap(), "clap");
        assert!(linux_output.is_file());
    }

    #[test]
    fn creates_audio_unit_metadata_and_standard_module_name() {
        let temp = TempDir::new();
        let binary = temp.path().join("libsource.dylib");
        write_macho(&binary, 0x0100_000c);
        let mut au = request(PackageFormat::Au, binary, temp.path().join("Audio Unit"));
        au.au = Some(AuMetadata {
            component_type: "aufx".into(),
            component_subtype: "test".into(),
            manufacturer: "SunM".into(),
            factory: "RustAUFactory".into(),
            sandbox_safe: true,
        });

        let output =
            package_for_target(&au, &Target::new(TargetPlatform::Macos, "aarch64")).unwrap();
        assert!(output.join("Contents/MacOS/Audio Unit").is_file());
        let plist = fs::read_to_string(output.join("Contents/Info.plist")).unwrap();
        assert!(plist.contains("<integer>66051</integer>"));
        assert!(plist.contains("<string>SunM</string>"));
        assert!(plist.contains("<true/>"));
    }

    #[test]
    fn validation_failure_preserves_existing_output() {
        let temp = TempDir::new();
        let binary = temp.path().join("source.so");
        write_elf(&binary, 62);
        let output = temp.path().join("Existing.vst3");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("sentinel"), "previous output").unwrap();

        let mut invalid = request(PackageFormat::Vst3, binary, output.clone());
        invalid.bundle_id = "invalid".into();
        let error = package_for_target(&invalid, &Target::new(TargetPlatform::Linux, "x86_64"))
            .unwrap_err();
        assert!(error.to_string().contains("bundle identifier"));
        assert_eq!(
            fs::read_to_string(output.join("sentinel")).unwrap(),
            "previous output"
        );
    }

    #[test]
    fn staging_failure_preserves_existing_output() {
        let temp = TempDir::new();
        let output = temp.path().join("Existing.vst3");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("sentinel"), "previous output").unwrap();

        let error = stage_and_publish(&output, |staging| {
            fs::create_dir(staging)?;
            fs::write(staging.join("partial"), "incomplete")?;
            bail!("simulated staging failure")
        })
        .unwrap_err();

        assert!(error.to_string().contains("simulated staging failure"));
        assert_eq!(
            fs::read_to_string(output.join("sentinel")).unwrap(),
            "previous output"
        );
        assert!(!temp.path().read_dir().unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("sunmao-stage")));
    }

    #[test]
    fn rejects_directories_wrong_module_kinds_and_wrong_architectures() {
        let temp = TempDir::new();
        let directory = temp.path().join("directory.so");
        fs::create_dir(&directory).unwrap();
        let directory_request = request(
            PackageFormat::Clap,
            directory,
            temp.path().join("DirectoryInput"),
        );
        assert!(package_for_target(
            &directory_request,
            &Target::new(TargetPlatform::Linux, "x86_64")
        )
        .unwrap_err()
        .to_string()
        .contains("regular file"));

        let executable = temp.path().join("executable.dll");
        let mut bytes = vec![0; 128];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
        bytes[64..68].copy_from_slice(b"PE\0\0");
        bytes[68..70].copy_from_slice(&0x8664u16.to_le_bytes());
        fs::write(&executable, bytes).unwrap();
        let executable_request = request(
            PackageFormat::Clap,
            executable,
            temp.path().join("Executable"),
        );
        let error = package_for_target(
            &executable_request,
            &Target::new(TargetPlatform::Windows, "x86_64"),
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("not marked as a DLL"));

        let wrong_arch = temp.path().join("wrong.so");
        write_elf(&wrong_arch, 183);
        let wrong_arch_request = request(
            PackageFormat::Vst3,
            wrong_arch,
            temp.path().join("WrongArch"),
        );
        let error = package_for_target(
            &wrong_arch_request,
            &Target::new(TargetPlatform::Linux, "x86_64"),
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("architecture"));
    }

    #[test]
    fn rejects_elf_class_mismatches_for_target_architecture() {
        let temp = TempDir::new();
        let binary = temp.path().join("wrong-class.so");
        write_elf_with_class(&binary, 1, 62);
        let package = request(PackageFormat::Clap, binary, temp.path().join("WrongClass"));

        let error = package_for_target(&package, &Target::new(TargetPlatform::Linux, "x86_64"))
            .unwrap_err();
        assert!(format!("{error:#}").contains("ELF class"));
    }

    #[test]
    fn rejects_elf_byte_order_mismatches_for_target_architecture() {
        let temp = TempDir::new();
        let binary = temp.path().join("wrong-endian.so");
        write_elf_with_class_and_encoding(&binary, 2, 62, 2);
        let package = request(PackageFormat::Clap, binary, temp.path().join("WrongEndian"));

        let error = package_for_target(&package, &Target::new(TargetPlatform::Linux, "x86_64"))
            .unwrap_err();
        assert!(format!("{error:#}").contains("ELF byte order"));
    }

    #[test]
    fn rejects_elf_pie_executables_with_an_interpreter_segment() {
        let temp = TempDir::new();
        let binary = temp.path().join("pie.so");
        write_elf_pie(&binary, 62);
        let package = request(
            PackageFormat::Clap,
            binary,
            temp.path().join("PieExecutable"),
        );

        let error = package_for_target(&package, &Target::new(TargetPlatform::Linux, "x86_64"))
            .unwrap_err();
        assert!(format!("{error:#}").contains("PIE executable"));
    }

    #[test]
    fn version_and_architecture_names_are_strict() {
        assert_eq!(parse_version("1.2.3").unwrap(), [1, 2, 3]);
        assert!(parse_version("1.2.3.4").is_err());
        assert!(parse_version("1.beta").is_err());
        assert!(version_to_au_integer([65536, 0, 0]).is_err());

        assert_eq!(
            vst3_architecture_directory(&Target::new(TargetPlatform::Windows, "aarch64")).unwrap(),
            "arm_64-win"
        );
        assert_eq!(
            vst3_architecture_directory(&Target::new(TargetPlatform::Linux, "x86")).unwrap(),
            "i386-linux"
        );
    }
}
