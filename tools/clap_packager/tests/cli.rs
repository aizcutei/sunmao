#[path = "../../packager_test_support.rs"]
mod support;

use std::process::Command;
use support::{TempDir, native_module};

#[test]
fn legacy_command_uses_the_unified_clap_layout() {
    let temp = TempDir::new("clap-packager-compat");
    let binary = native_module(&temp.path().join("source"));
    let output = temp.path().join("Compat CLAP");

    let result = Command::new(env!("CARGO_BIN_EXE_clap_packager"))
        .arg(&binary)
        .arg(&output)
        .args(["com.sunmao.compat-clap", "Compat CLAP", "1.2.3"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "compatibility command failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let plugin = output.with_extension("clap");
    #[cfg(target_os = "macos")]
    assert!(plugin.join("Contents/MacOS/Compat CLAP").is_file());
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    assert!(plugin.is_file());
}
