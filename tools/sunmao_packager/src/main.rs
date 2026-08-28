//! SunMao unified AU, VST3, CLAP, and standalone packager.

use anyhow::Result;
use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use sunmao_packager::{package, AuMetadata, PackageFormat, PackageRequest};

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum Format {
    /// Audio Unit (macOS only)
    Au,
    /// VST3 (cross-platform)
    Vst3,
    /// CLAP (cross-platform)
    Clap,
    /// Standalone application (cross-platform)
    Standalone,
}

impl From<Format> for PackageFormat {
    fn from(format: Format) -> Self {
        match format {
            Format::Au => Self::Au,
            Format::Vst3 => Self::Vst3,
            Format::Clap => Self::Clap,
            Format::Standalone => Self::Standalone,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "sunmao_packager")]
#[command(about = "Unified packager for AudioUnit, VST3, CLAP, and standalone artifacts")]
struct Args {
    /// Plugin format to package
    #[arg(value_enum)]
    format: Format,

    /// Path to the compiled module or standalone executable
    #[arg(long)]
    binary: PathBuf,

    /// Output path for the bundle/plugin
    #[arg(long)]
    out: PathBuf,

    /// Plugin name (human readable)
    #[arg(long)]
    name: String,

    /// Bundle identifier (e.g. com.vendor.plugin)
    #[arg(long)]
    bundle_id: String,

    /// Version string with one to three numeric components (e.g. 1.0.0)
    #[arg(long)]
    version: String,

    /// Perform ad-hoc code signing (macOS only)
    #[arg(long, default_value = "false")]
    codesign: bool,

    /// [AU only] Component type (4-char code, e.g. aufx)
    #[arg(long)]
    au_type: Option<String>,

    /// [AU only] Component subtype (4-char code, e.g. gain)
    #[arg(long)]
    au_subtype: Option<String>,

    /// [AU only] Manufacturer code (4-char code, e.g. Acme)
    #[arg(long)]
    au_manufacturer: Option<String>,

    /// [AU only] Factory function name
    #[arg(long, default_value = "RustAUFactory")]
    au_factory: String,

    /// [AU only] Sandbox safe
    #[arg(long, default_value = "true")]
    au_sandbox_safe: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let au = match args.format {
        Format::Au => Some(AuMetadata {
            component_type: args
                .au_type
                .clone()
                .ok_or_else(|| anyhow::anyhow!("--au-type is required for AU format"))?,
            component_subtype: args
                .au_subtype
                .clone()
                .ok_or_else(|| anyhow::anyhow!("--au-subtype is required for AU format"))?,
            manufacturer: args
                .au_manufacturer
                .clone()
                .ok_or_else(|| anyhow::anyhow!("--au-manufacturer is required for AU format"))?,
            factory: args.au_factory.clone(),
            sandbox_safe: args.au_sandbox_safe,
        }),
        Format::Vst3 | Format::Clap | Format::Standalone => None,
    };
    let format = args.format.into();
    let request = PackageRequest {
        format,
        binary: args.binary,
        out: args.out,
        name: args.name,
        bundle_id: args.bundle_id,
        version: args.version,
        codesign: args.codesign,
        au,
    };

    let output = package(&request)?;
    println!("Packaged {format:?}: {}", output.display());
    Ok(())
}
