//! Points the crate at the vanilla language files under `ref/` (built by `scripts/setup-ref.sh`).
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let lang = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../ref/src/assets/minecraft/lang");
    for (var, file) in [
        ("EN_US_JSON", "en_us.json"),
        ("DEPRECATED_JSON", "deprecated.json"),
    ] {
        let path = lang.join(file);
        if !path.exists() {
            panic!(
                "{} is missing: run scripts/setup-ref.sh (builds the gitignored ref/ reference material)",
                path.display()
            );
        }
        println!("cargo:rerun-if-changed={}", path.display());
        println!("cargo:rustc-env={var}={}", path.display());
    }
}
