fn main() {
    #[cfg(target_os = "macos")]
    {
        use std::fs;
        use std::path::PathBuf;

        let out_dir = std::env::var("OUT_DIR").unwrap_or_else(|_| "/tmp".to_string());
        let mut list_path = PathBuf::from(out_dir);
        list_path.push("exported_symbols.txt");

        let content = "_SunMaoDawInfoFactory\n_au_component_factory\n";
        let _ = fs::write(&list_path, content);

        println!(
            "cargo:rustc-link-arg=-Wl,-exported_symbols_list,{}",
            list_path.display()
        );
    }
}
