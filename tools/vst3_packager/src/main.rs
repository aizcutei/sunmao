//! VST3 Plugin Packager
//!
//! Creates .vst3 bundles for macOS, Windows, and Linux.

use clap::Parser;
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Parser)]
#[command(name = "vst3_packager")]
#[command(about = "Package VST3 plugins into bundles")]
struct Args {
    /// Path to the compiled binary (.dylib, .dll, or .so)
    #[arg(long)]
    binary: String,

    /// Output path for the .vst3 bundle
    #[arg(long)]
    out: String,

    /// Plugin name (used in Info.plist)
    #[arg(long)]
    name: String,

    /// Bundle identifier (e.g., com.vendor.plugin)
    #[arg(long)]
    bundle_id: String,

    /// Plugin version (e.g., 1.0.0)
    #[arg(long, default_value = "1.0.0")]
    version: String,

    /// Codesign the bundle (macOS only)
    #[arg(long, default_value = "false")]
    codesign: bool,
}

fn main() -> Result<(), String> {
    let args = Args::parse();

    let binary_path = Path::new(&args.binary);
    if !binary_path.exists() {
        return Err(format!("Binary not found: {}", args.binary));
    }

    let out_bundle = Path::new(&args.out);

    // Determine platform and create appropriate bundle structure
    #[cfg(target_os = "macos")]
    create_macos_bundle(&args, binary_path, out_bundle)?;

    #[cfg(target_os = "windows")]
    create_windows_bundle(&args, binary_path, out_bundle)?;

    #[cfg(target_os = "linux")]
    create_linux_bundle(&args, binary_path, out_bundle)?;

    Ok(())
}

#[cfg(target_os = "macos")]
fn create_macos_bundle(args: &Args, binary_path: &Path, out_bundle: &Path) -> Result<(), String> {
    // Remove existing bundle
    if out_bundle.exists() {
        fs::remove_dir_all(out_bundle).map_err(|e| e.to_string())?;
    }

    // Create bundle structure
    let contents_dir = out_bundle.join("Contents");
    let macos_dir = contents_dir.join("MacOS");
    let resources_dir = contents_dir.join("Resources");

    fs::create_dir_all(&macos_dir).map_err(|e| e.to_string())?;
    fs::create_dir_all(&resources_dir).map_err(|e| e.to_string())?;

    // Get the binary filename
    let binary_name = binary_path
        .file_name()
        .ok_or("Invalid binary path")?
        .to_str()
        .ok_or("Invalid binary filename")?;

    // Copy the binary
    let dest_binary = macos_dir.join(binary_name);
    fs::copy(binary_path, &dest_binary).map_err(|e| e.to_string())?;

    // Create Info.plist
    let plist_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>English</string>
    <key>CFBundleExecutable</key>
    <string>{}</string>
    <key>CFBundleIdentifier</key>
    <string>{}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>{}</string>
    <key>CFBundlePackageType</key>
    <string>BNDL</string>
    <key>CFBundleSignature</key>
    <string>????</string>
    <key>CFBundleVersion</key>
    <string>{}</string>
    <key>CFBundleShortVersionString</key>
    <string>{}</string>
</dict>
</plist>
"#,
        binary_name, args.bundle_id, args.name, args.version, args.version
    );

    fs::write(contents_dir.join("Info.plist"), plist_content).map_err(|e| e.to_string())?;

    // Create PkgInfo
    fs::write(contents_dir.join("PkgInfo"), "BNDL????").map_err(|e| e.to_string())?;

    // Codesign if requested
    if args.codesign {
        codesign_bundle(out_bundle)?;
    }

    println!("Created VST3 bundle: {}", out_bundle.display());
    Ok(())
}

#[cfg(target_os = "windows")]
fn create_windows_bundle(args: &Args, binary_path: &Path, out_bundle: &Path) -> Result<(), String> {
    // Remove existing bundle
    if out_bundle.exists() {
        fs::remove_dir_all(out_bundle).map_err(|e| e.to_string())?;
    }

    // Create bundle structure for Windows x64
    let contents_dir = out_bundle.join("Contents");
    let arch_dir = contents_dir.join("x86_64-win");

    fs::create_dir_all(&arch_dir).map_err(|e| e.to_string())?;

    // Get the binary filename, ensure .vst3 extension
    let binary_stem = binary_path
        .file_stem()
        .ok_or("Invalid binary path")?
        .to_str()
        .ok_or("Invalid binary filename")?;

    let dest_binary = arch_dir.join(format!("{}.vst3", binary_stem));
    fs::copy(binary_path, &dest_binary).map_err(|e| e.to_string())?;

    println!("Created VST3 bundle: {}", out_bundle.display());
    Ok(())
}

#[cfg(target_os = "linux")]
fn create_linux_bundle(args: &Args, binary_path: &Path, out_bundle: &Path) -> Result<(), String> {
    // Remove existing bundle
    if out_bundle.exists() {
        fs::remove_dir_all(out_bundle).map_err(|e| e.to_string())?;
    }

    // Create bundle structure for Linux x64
    let contents_dir = out_bundle.join("Contents");
    let arch_dir = contents_dir.join("x86_64-linux");

    fs::create_dir_all(&arch_dir).map_err(|e| e.to_string())?;

    // Get the binary filename
    let binary_name = binary_path
        .file_name()
        .ok_or("Invalid binary path")?
        .to_str()
        .ok_or("Invalid binary filename")?;

    let dest_binary = arch_dir.join(binary_name);
    fs::copy(binary_path, &dest_binary).map_err(|e| e.to_string())?;

    println!("Created VST3 bundle: {}", out_bundle.display());
    Ok(())
}

#[cfg(target_os = "macos")]
fn codesign_bundle(bundle_path: &Path) -> Result<(), String> {
    let status = Command::new("codesign")
        .args([
            "--force",
            "--sign",
            "-",
            "--deep",
            bundle_path.to_str().ok_or("Invalid bundle path")?,
        ])
        .status()
        .map_err(|e| format!("Failed to run codesign: {}", e))?;

    if !status.success() {
        return Err("Codesign failed".to_string());
    }

    Ok(())
}
