use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn usage() -> &'static str {
    "Usage: clap_packager <binary> <bundle-path> <bundle-id> <bundle-name> <version>"
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 6 {
        eprintln!("{}", usage());
        std::process::exit(2);
    }

    let binary = PathBuf::from(&args[1]);
    let bundle_path = PathBuf::from(&args[2]);
    let bundle_id = &args[3];
    let bundle_name = &args[4];
    let version = &args[5];

    if !binary.exists() {
        eprintln!("Binary does not exist: {}", binary.display());
        std::process::exit(2);
    }

    let executable_name = bundle_name;

    #[cfg(target_os = "macos")]
    {
        let contents_dir = bundle_path.join("Contents");
        let macos_dir = contents_dir.join("MacOS");
        fs::create_dir_all(&macos_dir)?;

        let target_binary = macos_dir.join(executable_name);
        fs::copy(&binary, &target_binary)?;

        let info_plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>English</string>
    <key>CFBundleExecutable</key>
    <string>{executable}</string>
    <key>CFBundleIdentifier</key>
    <string>{bundle_id}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>{bundle_name}</string>
    <key>CFBundlePackageType</key>
    <string>BNDL</string>
    <key>CFBundleSignature</key>
    <string>????</string>
    <key>CFBundleShortVersionString</key>
    <string>{version}</string>
    <key>CFBundleVersion</key>
    <string>{version}</string>
    <key>CSResourcesFileMapped</key>
    <true/>
</dict>
</plist>
"#,
            executable = executable_name,
            bundle_id = bundle_id,
            bundle_name = bundle_name,
            version = version
        );

        fs::write(contents_dir.join("Info.plist"), info_plist)?;
        fs::write(contents_dir.join("PkgInfo"), "BNDL????")?;

        let codesign_dir = contents_dir.join("_CodeSignature");
        fs::create_dir_all(&codesign_dir)?;
        fs::write(codesign_dir.join("CodeResources"), "")?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Some(parent) = bundle_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&binary, &bundle_path)?;
    }

    Ok(())
}
