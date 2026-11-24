fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        if let Ok(dir) = std::env::var("CARGO_MANIFEST_DIR") {
            let manifest = std::path::Path::new(&dir).join("windows.manifest");
            println!("cargo:rerun-if-changed={}", manifest.display());
            println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
            println!(
                "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
                manifest.display()
            );
        }
    }
}
