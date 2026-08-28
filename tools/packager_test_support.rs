use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "sunmao-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn native_module(base: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let path = base.with_extension("dylib");
        let cpu_type = match std::env::consts::ARCH {
            "x86_64" => 0x0100_0007u32,
            "aarch64" => 0x0100_000cu32,
            architecture => panic!("unsupported test architecture {architecture}"),
        };
        let mut bytes = vec![0; 32];
        bytes[..4].copy_from_slice(&[0xcf, 0xfa, 0xed, 0xfe]);
        bytes[4..8].copy_from_slice(&cpu_type.to_le_bytes());
        bytes[12..16].copy_from_slice(&6u32.to_le_bytes());
        fs::write(&path, bytes).unwrap();
        path
    }

    #[cfg(target_os = "linux")]
    {
        let path = base.with_extension("so");
        let (class, machine) = match std::env::consts::ARCH {
            "x86" => (1, 3u16),
            "x86_64" => (2, 62u16),
            "arm" => (1, 40u16),
            "aarch64" => (2, 183u16),
            "riscv64" => (2, 243u16),
            architecture => panic!("unsupported test architecture {architecture}"),
        };
        let header_len = if class == 1 { 52 } else { 64 };
        let mut bytes = vec![0; header_len];
        bytes[..4].copy_from_slice(b"\x7fELF");
        bytes[4] = class;
        bytes[5] = 1;
        bytes[16..18].copy_from_slice(&3u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&machine.to_le_bytes());
        fs::write(&path, bytes).unwrap();
        path
    }

    #[cfg(target_os = "windows")]
    {
        let path = base.with_extension("dll");
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
        fs::write(&path, bytes).unwrap();
        path
    }
}
