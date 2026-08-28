//! Compatibility entry point for the original positional CLAP packager.
//!
//! New projects should invoke `sunmao_packager clap` directly.  Keeping this
//! small shim avoids having two implementations of bundle layout and, most
//! importantly, gives callers the unified packager's input validation and
//! staged publication guarantees.

use std::path::PathBuf;
use std::process::ExitCode;
use sunmao_packager::{PackageFormat, PackageRequest, package};

fn usage() -> &'static str {
    "Usage: clap_packager <binary> <bundle-path> <bundle-id> <bundle-name> <version>"
}

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 6 {
        eprintln!("{usage}", usage = usage());
        return ExitCode::from(2);
    }

    let request = PackageRequest {
        format: PackageFormat::Clap,
        binary: PathBuf::from(&args[1]),
        out: PathBuf::from(&args[2]),
        name: args[4].to_string_lossy().into_owned(),
        bundle_id: args[3].to_string_lossy().into_owned(),
        version: args[5].to_string_lossy().into_owned(),
        codesign: false,
        au: None,
    };

    match package(&request) {
        Ok(output) => {
            println!("Packaged CLAP: {}", output.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("CLAP packaging failed: {error:#}");
            ExitCode::FAILURE
        }
    }
}
