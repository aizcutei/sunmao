//! Compatibility entry point for the original AudioUnit packager.
//!
//! AU is intentionally outside the Phase 1 gate, but this command remains
//! useful for macOS experiments.  All validation, plist generation, signing,
//! and publication are delegated to `sunmao_packager` so the legacy command
//! cannot silently overwrite a partially-built component.

use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;
use sunmao_packager::{AuMetadata, PackageFormat, PackageRequest, package};

#[derive(Parser, Debug)]
#[command(name = "au_packager")]
#[command(about = "Compatibility wrapper for sunmao_packager au (macOS only)")]
struct Args {
    /// Path to the compiled macOS dynamic library
    #[arg(long)]
    binary: PathBuf,

    /// Output path for the .component bundle
    #[arg(long)]
    out: PathBuf,

    /// Human-readable component name
    #[arg(long)]
    name: String,

    /// Bundle identifier
    #[arg(long)]
    bundle_id: String,

    /// Four-character AudioComponent type (for example `aufx`)
    #[arg(long = "type")]
    component_type: String,

    /// Four-character AudioComponent subtype
    #[arg(long = "subtype")]
    component_subtype: String,

    /// Four-character manufacturer code
    #[arg(long = "manufacturer")]
    component_manufacturer: String,

    /// Semantic version (`major.minor.patch`) or legacy packed integer/hex
    #[arg(long)]
    version: String,

    /// Factory function symbol
    #[arg(long, default_value = "RustAUFactory")]
    factory: String,

    /// Keep the legacy default of a sandbox-safe component
    #[arg(long)]
    sandbox_safe: bool,

    /// Mark the component as not sandbox safe
    #[arg(long = "unsafe")]
    unsafe_mode: bool,

    /// Legacy executable override. The unified packager derives this from
    /// `--out`; accepting only the matching value avoids producing a bundle
    /// whose Info.plist points at a different file.
    #[arg(long)]
    executable: Option<String>,

    /// Ad-hoc codesign the component
    #[arg(long)]
    codesign: bool,

    /// Legacy install option. Installation is intentionally handled by the
    /// repository packaging helper, which provides transactional publication.
    #[arg(long)]
    install: Option<PathBuf>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match package_au(args) {
        Ok(output) => {
            println!("Packaged AU: {}", output.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("AU packaging failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn package_au(args: Args) -> Result<PathBuf, String> {
    if args.sandbox_safe && args.unsafe_mode {
        return Err("--sandbox-safe and --unsafe are mutually exclusive".into());
    }
    if args.install.is_some() {
        return Err(
            "--install is no longer performed by the compatibility wrapper; use tools/package_examples.sh --au --install"
                .into(),
        );
    }

    let version = normalize_version(&args.version)?;
    let output = args.out.with_extension("component");
    if let Some(executable) = args.executable {
        let expected = output
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| "--out must have a valid UTF-8 filename".to_string())?;
        if executable != expected {
            return Err(format!(
                "--executable '{executable}' does not match the unified bundle module name '{expected}'; rename --out or omit --executable"
            ));
        }
    }

    let request = PackageRequest {
        format: PackageFormat::Au,
        binary: args.binary,
        out: args.out,
        name: args.name,
        bundle_id: args.bundle_id,
        version,
        codesign: args.codesign,
        au: Some(AuMetadata {
            component_type: args.component_type,
            component_subtype: args.component_subtype,
            manufacturer: args.component_manufacturer,
            factory: args.factory,
            sandbox_safe: !args.unsafe_mode,
        }),
    };

    package(&request).map_err(|error| format!("{error:#}"))
}

fn normalize_version(raw: &str) -> Result<String, String> {
    if raw.contains('.') {
        return Ok(raw.to_owned());
    }

    let trimmed = raw.trim().replace('_', "");
    let packed = if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u32::from_str_radix(hex, 16)
    } else {
        trimmed.parse::<u32>()
    }
    .map_err(|error| format!("invalid packed AU version '{raw}': {error}"))?;

    let major = packed >> 16;
    let minor = (packed >> 8) & 0xff;
    let patch = packed & 0xff;
    Ok(format!("{major}.{minor}.{patch}"))
}

#[cfg(test)]
mod tests {
    use super::normalize_version;

    #[test]
    fn accepts_semantic_versions() {
        assert_eq!(normalize_version("1.2.3").unwrap(), "1.2.3");
    }

    #[test]
    fn converts_legacy_packed_versions() {
        assert_eq!(normalize_version("0x00010203").unwrap(), "1.2.3");
        assert_eq!(normalize_version("65536").unwrap(), "1.0.0");
    }
}
