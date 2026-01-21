//! SunMao Unified Packager
//! 
//! A comprehensive tool for packaging audio plugins (AudioUnit, VST3, CLAP) for macOS, Windows, and Linux.
//! Handles bundle creation, plist generation, and code signing.

use clap::{Parser, ValueEnum};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Context, Result, bail};

/// Supported plugin formats
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum Format {
    /// Audio Unit (macOS only)
    Au,
    /// VST3 (Cross-platform)
    Vst3,
    /// CLAP (Cross-platform)
    Clap,
}

#[derive(Parser, Debug)]
#[command(name = "sunmao_packager")]
#[command(about = "Unified packager for AudioUnit, VST3, and CLAP plugins", long_about = None)]
struct Args {
    /// Plugin format to package
    #[arg(value_enum)]
    format: Format,

    /// Path to the compiled binary (.dylib, .dll, .so)
    #[arg(long)]
    binary: PathBuf,

    /// Output path for the bundle/plugin
    #[arg(long)]
    out: PathBuf,

    /// Plugin Name (human readable)
    #[arg(long)]
    name: String,

    /// Bundle Identifier (e.g., com.vendor.plugin)
    #[arg(long)]
    bundle_id: String,

    /// Version string (e.g., 1.0.0)
    #[arg(long)]
    version: String,

    /// Perform code signing (macOS only)
    #[arg(long, default_value = "false")]
    codesign: bool,

    // --- AU Specific Arguments ---

    /// [AU Only] Component Type (4-char code, e.g., 'aufx')
    #[arg(long, requires = "au_subtype")]
    au_type: Option<String>,

    /// [AU Only] Component Subtype (4-char code, e.g., 'gain')
    #[arg(long)]
    au_subtype: Option<String>,

    /// [AU Only] Manufacturer Code (4-char code, e.g., 'Acme')
    #[arg(long)]
    au_manufacturer: Option<String>,

    /// [AU Only] Factory Function Name
    #[arg(long, default_value = "RustAUFactory")]
    au_factory: String,

    /// [AU Only] Sandbox Safe (default: true)
    #[arg(long, default_value = "true")]
    au_sandbox_safe: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if !args.binary.exists() {
        bail!("Input binary does not exist: {}", args.binary.display());
    }

    match args.format {
        Format::Au => package_au(&args)?,
        Format::Vst3 => package_vst3(&args)?,
        Format::Clap => package_clap(&args)?,
    }

    Ok(())
}

// =============================================================================
// Audio Unit Packaging (macOS Only)
// =============================================================================

fn package_au(args: &Args) -> Result<()> {
    #[cfg(not(target_os = "macos"))]
    {
        bail!("AudioUnit packaging is only supported on macOS.");
    }

    #[cfg(target_os = "macos")]
    {
        // Validate AU specific args
        let au_type = args.au_type.as_deref().context("--au-type is required for AU format")?;
        let au_subtype = args.au_subtype.as_deref().context("--au-subtype is required for AU format")?;
        let au_mfr = args.au_manufacturer.as_deref().context("--au-manufacturer is required for AU format")?;

        // Prepare output path (.component)
        let out_bundle = if args.out.extension().map_or(false, |e| e == "component") {
            args.out.clone()
        } else {
            args.out.with_extension("component")
        };

        println!("Packaging AudioUnit: {}", out_bundle.display());

        // Create Bundle Structure
        // MyPlugin.component/
        //   Contents/
        //     MacOS/
        //     Resources/
        //     Info.plist
        //     PkgInfo
        let contents_dir = out_bundle.join("Contents");
        let macos_dir = contents_dir.join("MacOS");
        let resources_dir = contents_dir.join("Resources");

        if out_bundle.exists() {
            fs::remove_dir_all(&out_bundle).context("Failed to clean output directory")?;
        }
        fs::create_dir_all(&macos_dir).context("Failed to create MacOS directory")?;
        fs::create_dir_all(&resources_dir).context("Failed to create Resources directory")?;

        // Copy Binary
        let binary_name = args.binary.file_name().context("Invalid binary filename")?;
        fs::copy(&args.binary, macos_dir.join(binary_name)).context("Failed to copy binary")?;

        // Generate Info.plist
        // We handle the version parsing logic (1.0.0 -> 0x...) inside the generation or just use raw string if simple.
        // AU requires a specific integer version format often, but standard parsing is:
        // MAJOR.MINOR.PATCH -> (Major << 16) | (Minor << 8) | Patch
        let version_int = parse_version_to_int(&args.version)?;
        
        // AU Info.plist Template
        let plist_content = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
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
            <string>{mfr}</string>
            <key>name</key>
            <string>{name}</string>
            <key>sandboxSafe</key>
            <{sandbox}/>
            <key>subtype</key>
            <string>{subtype}</string>
            <key>type</key>
            <string>{type}</string>
            <key>version</key>
            <integer>{version_int}</integer>
        </dict>
    </array>
</dict>
</plist>"#,
            exec_name = binary_name.to_string_lossy(),
            bundle_id = args.bundle_id,
            name = args.name,
            version = args.version,
            factory = args.au_factory,
            mfr = au_mfr,
            sandbox = if args.au_sandbox_safe { "true" } else { "false" },
            subtype = au_subtype,
            type = au_type,
            version_int = version_int
        );

        fs::write(contents_dir.join("Info.plist"), plist_content).context("Failed to write Info.plist")?;
        fs::write(contents_dir.join("PkgInfo"), "BNDL????").context("Failed to write PkgInfo")?;

        if args.codesign {
            codesign_bundle(&out_bundle)?;
        }

        Ok(())
    }
}

// =============================================================================
// VST3 Packaging (Cross-platform)
// =============================================================================

fn package_vst3(args: &Args) -> Result<()> {
    // Determine extension
    let out_bundle = if args.out.extension().map_or(false, |e| e == "vst3") {
        args.out.clone()
    } else {
        args.out.with_extension("vst3")
    };
    
    println!("Packaging VST3: {}", out_bundle.display());

    // Clean existing
    if out_bundle.exists() {
        fs::remove_dir_all(&out_bundle).context("Failed to clean output directory")?;
    }

    let contents_dir = out_bundle.join("Contents");

    // Platform Specific Structure
    #[cfg(target_os = "macos")]
    {
        // macOS: standard Bundle structure
        // MyPlugin.vst3/Contents/MacOS/MyPlugin
        let macos_dir = contents_dir.join("MacOS");
        let resources_dir = contents_dir.join("Resources");
        
        fs::create_dir_all(&macos_dir).context("Failed to create MacOS directory")?;
        fs::create_dir_all(&resources_dir).context("Failed to create Resources directory")?;

        let binary_name = args.binary.file_name().context("Invalid binary filename")?;
        fs::copy(&args.binary, macos_dir.join(binary_name)).context("Failed to copy binary")?;

        // VST3 Info.plist
        let plist_content = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
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
</plist>"#,
            exec_name = binary_name.to_string_lossy(),
            bundle_id = args.bundle_id,
            name = args.name,
            version = args.version
        );

        fs::write(contents_dir.join("Info.plist"), plist_content).context("Failed to write Info.plist")?;
        fs::write(contents_dir.join("PkgInfo"), "BNDL????").context("Failed to write PkgInfo")?;

        if args.codesign {
            codesign_bundle(&out_bundle)?;
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Windows: MyPlugin.vst3/Contents/x86_64-win/MyPlugin.vst3
        // Note: The binary itself is usually renamed to .vst3
        let arch_dir = contents_dir.join("x86_64-win");
        fs::create_dir_all(&arch_dir).context("Failed to create arch directory")?;

        let stem = args.binary.file_stem().context("Invalid binary name")?;
        let dest_name = format!("{}.vst3", stem.to_string_lossy());
        fs::copy(&args.binary, arch_dir.join(dest_name)).context("Failed to copy binary")?;
    }

    #[cfg(target_os = "linux")]
    {
        // Linux: MyPlugin.vst3/Contents/x86_64-linux/MyPlugin.so
        let arch_dir = contents_dir.join("x86_64-linux");
        fs::create_dir_all(&arch_dir).context("Failed to create arch directory")?;

        let binary_name = args.binary.file_name().context("Invalid binary name")?;
        fs::copy(&args.binary, arch_dir.join(binary_name)).context("Failed to copy binary")?;
    }

    Ok(())
}

// =============================================================================
// CLAP Packaging (Cross-platform)
// =============================================================================

fn package_clap(args: &Args) -> Result<()> {
    // Check if output is meant to be a bundle (ends in .clap but is a folder on macOS)
    // On macOS, CLAP *must* be a bundle.
    // On Windows/Linux, it CAN be a bundle, but usually just a single file (.clap).
    
    #[cfg(target_os = "macos")]
    {
        let out_bundle = if args.out.extension().map_or(false, |e| e == "clap") {
            args.out.clone()
        } else {
            args.out.with_extension("clap")
        };
        
        println!("Packaging CLAP Bundle (macOS): {}", out_bundle.display());
        
        if out_bundle.exists() {
            fs::remove_dir_all(&out_bundle).context("Failed to clean output directory")?;
        }

        let contents_dir = out_bundle.join("Contents");
        let macos_dir = contents_dir.join("MacOS");
        fs::create_dir_all(&macos_dir).context("Failed to create MacOS directory")?;

        let binary_name = args.binary.file_name().context("Invalid binary filename")?;
        fs::copy(&args.binary, macos_dir.join(binary_name)).context("Failed to copy binary")?;

        // CLAP Info.plist
        let plist_content = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
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
    <key>CSResourcesFileMapped</key>
    <true/>
</dict>
</plist>"#,
            exec_name = binary_name.to_string_lossy(),
            bundle_id = args.bundle_id,
            name = args.name,
            version = args.version
        );

        fs::write(contents_dir.join("Info.plist"), plist_content).context("Failed to write Info.plist")?;
        fs::write(contents_dir.join("PkgInfo"), "BNDL????").context("Failed to write PkgInfo")?;

        // CodeSignature structure (empty placeholder often needed before signing)
        let codesign_dir = contents_dir.join("_CodeSignature");
        fs::create_dir_all(&codesign_dir)?;
        fs::write(codesign_dir.join("CodeResources"), "")?;

        if args.codesign {
            codesign_bundle(&out_bundle)?;
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Linux/Windows: Just copy and rename to .clap
        // If output path is a directory, verify arguments or assume filename from binary
        
        let out_file = if args.out.extension().map_or(false, |e| e == "clap") {
            args.out.clone()
        } else {
            args.out.with_extension("clap")
        };

        println!("Packaging CLAP (Single File): {}", out_file.display());
        
        if let Some(parent) = out_file.parent() {
            fs::create_dir_all(parent).context("Failed to create output directory")?;
        }

        fs::copy(&args.binary, &out_file).context("Failed to copy binary")?;
    }

    Ok(())
}

// =============================================================================
// Helper Functions
// =============================================================================

#[cfg(target_os = "macos")]
fn codesign_bundle(bundle_path: &Path) -> Result<()> {
    println!("Codesigning: {}", bundle_path.display());
    let status = Command::new("codesign")
        .args([
            "--force",
            "--sign", "-", // ad-hoc signature
            "--deep",
            bundle_path.to_str().context("Invalid bundle path")?,
        ])
        .status()
        .context("Failed to execute codesign")?;

    if !status.success() {
        bail!("codesign failed with status: {}", status);
    }
    Ok(())
}

fn parse_version_to_int(version: &str) -> Result<u32> {
    // Format: X.Y.Z -> (X << 16) | (Y << 8) | Z
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() < 1 {
        bail!("Invalid version string");
    }
    
    let major = parts[0].parse::<u32>().unwrap_or(0);
    let minor = if parts.len() > 1 { parts[1].parse::<u32>().unwrap_or(0) } else { 0 };
    let patch = if parts.len() > 2 { parts[2].parse::<u32>().unwrap_or(0) } else { 0 };
    
    Ok((major << 16) | (minor << 8) | patch)
}
