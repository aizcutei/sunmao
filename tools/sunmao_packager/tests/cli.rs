use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "sunmao-packager-cli-{}-{}",
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

fn native_binary(path: &Path) {
    #[cfg(target_os = "macos")]
    {
        let cpu_type = match std::env::consts::ARCH {
            "x86_64" => 0x0100_0007u32,
            "aarch64" => 0x0100_000cu32,
            architecture => panic!("unsupported test architecture {architecture}"),
        };
        let mut bytes = vec![0; 32];
        bytes[..4].copy_from_slice(&[0xcf, 0xfa, 0xed, 0xfe]);
        bytes[4..8].copy_from_slice(&cpu_type.to_le_bytes());
        bytes[12..16].copy_from_slice(&6u32.to_le_bytes());
        fs::write(path.with_extension("dylib"), bytes).unwrap();
    }

    #[cfg(target_os = "linux")]
    {
        let machine = match std::env::consts::ARCH {
            "x86" => 3u16,
            "x86_64" => 62u16,
            "arm" => 40u16,
            "aarch64" => 183u16,
            "riscv64" => 243u16,
            architecture => panic!("unsupported test architecture {architecture}"),
        };
        let mut bytes = vec![0; 64];
        bytes[..4].copy_from_slice(b"\x7fELF");
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[16..18].copy_from_slice(&3u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&machine.to_le_bytes());
        fs::write(path.with_extension("so"), bytes).unwrap();
    }

    #[cfg(target_os = "windows")]
    {
        let machine = match std::env::consts::ARCH {
            "x86" => 0x014cu16,
            "x86_64" => 0x8664u16,
            "aarch64" => 0xaa64u16,
            architecture => panic!("unsupported test architecture {architecture}"),
        };
        let mut bytes = vec![0; 128];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
        bytes[64..68].copy_from_slice(b"PE\0\0");
        bytes[68..70].copy_from_slice(&machine.to_le_bytes());
        bytes[86..88].copy_from_slice(&0x2000u16.to_le_bytes());
        fs::write(path.with_extension("dll"), bytes).unwrap();
    }
}

fn native_binary_path(base: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    return base.with_extension("dylib");
    #[cfg(target_os = "linux")]
    return base.with_extension("so");
    #[cfg(target_os = "windows")]
    return base.with_extension("dll");
}

#[test]
fn cli_packages_a_native_vst3_bundle() {
    let temp = TempDir::new();
    let binary_base = temp.path().join("source");
    native_binary(&binary_base);
    let binary = native_binary_path(&binary_base);
    let output_base = temp.path().join("CLI Product");

    let result = Command::new(env!("CARGO_BIN_EXE_sunmao_packager"))
        .args(["vst3", "--binary"])
        .arg(&binary)
        .arg("--out")
        .arg(&output_base)
        .args([
            "--name",
            "CLI Product",
            "--bundle-id",
            "com.sunmao.cli-product",
            "--version",
            "1.0.0",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "packager failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let bundle = output_base.with_extension("vst3");
    #[cfg(target_os = "macos")]
    assert!(bundle.join("Contents/MacOS/CLI Product").is_file());
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    assert!(bundle
        .join("Contents/x86_64-linux/CLI Product.so")
        .is_file());
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    assert!(bundle
        .join("Contents/x86_64-win/CLI Product.vst3")
        .is_file());
}

#[test]
fn cli_packages_a_real_native_standalone_executable() {
    let temp = TempDir::new();
    let binary = std::env::current_exe().unwrap();
    let output_base = temp.path().join("CLI Standalone");

    let result = Command::new(env!("CARGO_BIN_EXE_sunmao_packager"))
        .args(["standalone", "--binary"])
        .arg(&binary)
        .arg("--out")
        .arg(&output_base)
        .args([
            "--name",
            "CLI Standalone",
            "--bundle-id",
            "com.sunmao.cli-standalone",
            "--version",
            "1.0.0",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "packager failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    #[cfg(target_os = "macos")]
    {
        let app = output_base.with_extension("app");
        assert!(app.join("Contents/MacOS/CLI Standalone").is_file());
        let plist = fs::read_to_string(app.join("Contents/Info.plist")).unwrap();
        assert!(plist.contains("<string>APPL</string>"));
    }
    #[cfg(target_os = "linux")]
    assert!(output_base.is_file());
    #[cfg(target_os = "windows")]
    assert!(output_base.with_extension("exe").is_file());
}

#[test]
fn cli_validation_does_not_replace_an_existing_bundle() {
    let temp = TempDir::new();
    let binary_base = temp.path().join("source");
    native_binary(&binary_base);
    let binary = native_binary_path(&binary_base);
    let output_base = temp.path().join("Existing");
    let bundle = output_base.with_extension("vst3");
    fs::create_dir(&bundle).unwrap();
    fs::write(bundle.join("sentinel"), "keep me").unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_sunmao_packager"))
        .args(["vst3", "--binary"])
        .arg(&binary)
        .arg("--out")
        .arg(&output_base)
        .args([
            "--name",
            "Existing",
            "--bundle-id",
            "invalid",
            "--version",
            "1.0.0",
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(
        fs::read_to_string(bundle.join("sentinel")).unwrap(),
        "keep me"
    );
}
