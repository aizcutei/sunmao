use std::env;

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_SYSTEM_CAPTURE");
    println!("cargo:rerun-if-env-changed=MACOSX_DEPLOYMENT_TARGET");

    // ScreenCaptureKit's Swift bridge emits an @rpath dependency on
    // libswift_Concurrency. Its dependency build script cannot propagate the
    // rpath to this crate's test/binary targets, so add the system Swift
    // runtime location at the final link step. Do not add an Xcode toolchain
    // path: mixing it with the OS Swift runtime registers duplicate classes.
    if env::var_os("CARGO_FEATURE_SYSTEM_CAPTURE").is_some()
        && env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
    {
        // ScreenCaptureKit itself is available from macOS 12.3, but the
        // screencapturekit Swift bridge declares macOS 13 as its minimum. A
        // lower final target selects Swift's back-deployment concurrency
        // runtime, which is not packaged by Cargo and can be mixed with the OS
        // runtime if callers point DYLD_LIBRARY_PATH at Xcode.
        let deployment_target = env::var("MACOSX_DEPLOYMENT_TARGET").unwrap_or_default();
        let supported = deployment_target
            .split('.')
            .next()
            .and_then(|major| major.parse::<u32>().ok())
            .is_some_and(|major| major >= 13);
        assert!(
            supported,
            "the `system-capture` feature requires MACOSX_DEPLOYMENT_TARGET=13.0 or newer for the final application (got '{}')",
            if deployment_target.is_empty() {
                "unset"
            } else {
                &deployment_target
            }
        );

        println!("cargo:rustc-link-arg=-mmacosx-version-min=13.0");
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }
}
