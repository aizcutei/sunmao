//! Compatibility entry point for the original VST3 packager.
//!
//! The implementation lives in `sunmao_packager`; this crate only preserves
//! the old command name and flags for existing scripts.

use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;
use sunmao_packager::{PackageFormat, PackageRequest, package};

#[derive(Parser, Debug)]
#[command(name = "vst3_packager")]
#[command(about = "Compatibility wrapper for sunmao_packager vst3")]
struct Args {
    /// Path to the compiled binary (.dylib, .dll, or .so)
    #[arg(long)]
    binary: PathBuf,

    /// Output path for the .vst3 bundle
    #[arg(long)]
    out: PathBuf,

    /// Plugin name (used in Info.plist on macOS)
    #[arg(long)]
    name: String,

    /// Bundle identifier (e.g. com.vendor.plugin)
    #[arg(long)]
    bundle_id: String,

    /// Plugin version (e.g. 1.0.0)
    #[arg(long, default_value = "1.0.0")]
    version: String,

    /// Ad-hoc codesign the bundle (macOS only)
    #[arg(long)]
    codesign: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let request = PackageRequest {
        format: PackageFormat::Vst3,
        binary: args.binary,
        out: args.out,
        name: args.name,
        bundle_id: args.bundle_id,
        version: args.version,
        codesign: args.codesign,
        au: None,
    };

    match package(&request) {
        Ok(output) => {
            println!("Packaged VST3: {}", output.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("VST3 packaging failed: {error:#}");
            ExitCode::FAILURE
        }
    }
}
