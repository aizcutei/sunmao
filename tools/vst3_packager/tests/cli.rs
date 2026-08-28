#[path = "../../packager_test_support.rs"]
mod support;

use std::process::Command;
use support::{TempDir, native_module};

#[test]
fn legacy_command_uses_the_unified_vst3_layout() {
    let temp = TempDir::new("vst3-packager-compat");
    let binary = native_module(&temp.path().join("source"));
    let output = temp.path().join("Compat VST3");

    let result = Command::new(env!("CARGO_BIN_EXE_vst3_packager"))
        .arg("--binary")
        .arg(&binary)
        .arg("--out")
        .arg(&output)
        .args([
            "--name",
            "Compat VST3",
            "--bundle-id",
            "com.sunmao.compat-vst3",
            "--version",
            "1.2.3",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "compatibility command failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let bundle = output.with_extension("vst3");
    #[cfg(target_os = "macos")]
    assert!(bundle.join("Contents/MacOS/Compat VST3").is_file());
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    assert!(
        bundle
            .join("Contents/x86_64-linux/Compat VST3.so")
            .is_file()
    );
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    assert!(
        bundle
            .join("Contents/x86_64-win/Compat VST3.vst3")
            .is_file()
    );
}
