#![allow(dead_code)]
use serde_json::Value;
use std::path::PathBuf;

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn read_json(rel: &str) -> Value {
    let p = repo_root().join(rel);
    let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    serde_json::from_str(&text).unwrap()
}

pub fn golden() -> Vec<Value> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/block_golden.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}
