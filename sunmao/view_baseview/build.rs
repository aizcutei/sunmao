fn main() {
    // Build scripts run for the host, so use Cargo's target OS instead of cfg!.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
        && std::env::var_os("CARGO_FEATURE_WEBVIEW").is_some()
    {
        println!("cargo:rustc-link-lib=framework=WebKit");
    }
}
