//! Points the crate at the vanilla management-protocol schema under `ref/` (built by `scripts/setup-ref.sh`).
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let path = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../ref/generated/reports/json-rpc-api-schema.json");
    if !path.exists() {
        panic!(
            "{} is missing: run scripts/setup-ref.sh (builds the gitignored ref/ reference material)",
            path.display()
        );
    }
    println!("cargo:rerun-if-changed={}", path.display());
    println!("cargo:rustc-env=JSON_RPC_API_SCHEMA={}", path.display());
}
