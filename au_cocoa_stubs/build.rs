use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=src/cocoa_stub.m");
    println!("cargo:rerun-if-env-changed=MACOSX_DEPLOYMENT_TARGET");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    let target_arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        Ok(arch) => panic!("unsupported macOS architecture for Cocoa stubs: {arch}"),
        Err(error) => panic!("CARGO_CFG_TARGET_ARCH not set: {error}"),
    };
    let default_deployment_target = if target_arch == "arm64" {
        "11.0"
    } else {
        "10.12"
    };
    let deployment_target = env::var("MACOSX_DEPLOYMENT_TARGET")
        .unwrap_or_else(|_| default_deployment_target.to_owned());

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR not set"));
    let source = out_dir.join("cocoa_stub.m");
    fs::write(&source, include_bytes!("src/cocoa_stub.m"))
        .expect("failed to materialize Cocoa stub source");

    let object = out_dir.join("cocoa_stub.o");
    let status = Command::new("clang")
        .args(["-fobjc-arc", "-arch", target_arch])
        .arg(format!("-mmacosx-version-min={deployment_target}"))
        .args(["-c", "-o"])
        .arg(&object)
        .arg(&source)
        .status()
        .expect("failed to run clang");
    assert!(status.success(), "clang compilation failed");

    let archive = out_dir.join("libcocoa_stub.a");
    let status = Command::new("libtool")
        .args(["-static", "-o"])
        .arg(&archive)
        .arg(&object)
        .status()
        .expect("failed to run libtool");
    assert!(status.success(), "libtool failed");

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static:+whole-archive=cocoa_stub");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=dylib=objc");
}
