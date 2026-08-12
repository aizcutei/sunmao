fn main() {
    // `build.rs` is compiled for and executed on the host, so a Rust
    // `#[cfg(target_os = ...)]` here describes the host rather than the
    // plugin target. Read Cargo's target-specific environment instead.
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_OS");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_AU");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    if std::env::var_os("CARGO_FEATURE_AU").is_none() {
        return;
    }

    use std::fs;
    use std::path::PathBuf;

    let out_dir = std::env::var("OUT_DIR").unwrap_or_else(|_| "/tmp".to_string());
    let mut list_path = PathBuf::from(out_dir);
    list_path.push("exported_symbols.txt");

    let content = "_RustAUFactory\n_au_component_factory\n";
    let _ = fs::write(&list_path, content);

    println!(
        "cargo:rustc-link-arg=-Wl,-exported_symbols_list,{}",
        list_path.display()
    );
}
