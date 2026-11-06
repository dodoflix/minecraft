//! Helpers of the `region_*` oracle tests: the vanilla-java runner and the vanilla dump of a region folder.
#![allow(
    dead_code,
    reason = "each test target uses a subset of the shared helpers"
)]

use crate::region_common::{fixtures, repo_root};
use std::path::Path;
use std::process::Command;

/// Runs `scripts/vanilla-java.sh <program> <args>`; panics with the fix when the script, `ref/bundle` or `java` is missing.
pub fn run_java(program: &str, args: &[&str]) -> String {
    let root = repo_root();
    let script = root.join("scripts/vanilla-java.sh");
    assert!(
        script.exists() && root.join("ref/bundle").exists(),
        "scripts/vanilla-java.sh or ref/bundle is missing: run scripts/setup-ref.sh"
    );
    assert!(
        Command::new("java").arg("-version").output().is_ok(),
        "java not found on PATH: install JDK 25"
    );
    let out = Command::new(script)
        .arg(fixtures().join(program))
        .args(args)
        .output()
        .expect("spawn vanilla-java.sh");
    assert!(
        out.status.success(),
        "{program} {args:?} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// Vanilla `dump` output restricted to `has=true` slots.
pub fn java_dump(dir: &Path) -> Vec<String> {
    run_java("RegionTool.java", &["dump", dir.to_str().unwrap()])
        .lines()
        .filter(|l| l.contains(" has=true "))
        .map(str::to_owned)
        .collect()
}

/// Copies the `.mca`/`.mcc` files of `src` into a fresh directory.
pub fn copy_region_files(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_file() {
            std::fs::copy(e.path(), dst.join(e.file_name())).unwrap();
        }
    }
}
