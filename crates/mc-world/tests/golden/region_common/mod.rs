//! Shared helpers for the `region_*` integration tests: repo paths, a vanilla-java runner, SHA-256
//! and a dump of region files in the same line format as `fixtures/region/RegionTool.java dump`.
#![allow(dead_code, reason = "each test binary uses a subset of the shared helpers")]

use mc_core::pos::chunk_pos::ChunkPos;
use mc_core::ident::Identifier;
use mc_world::region::{RegionFile, RegionStorageInfo};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn repo_root() -> PathBuf {
    match std::env::var_os("MC_REPO_ROOT") {
        Some(p) => PathBuf::from(p),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
    }
}

pub fn fixtures() -> PathBuf {
    repo_root().join("crates/mc-world/tests/fixtures/region")
}

pub fn info() -> RegionStorageInfo {
    RegionStorageInfo::new("test", Identifier::parse("minecraft:overworld").unwrap(), "chunk")
}

/// Runs `scripts/vanilla-java.sh <program> <args>`; `None` (after printing why) when the vanilla
/// oracle is unavailable so tests skip instead of failing on a clean checkout.
pub fn run_java(program: &str, args: &[&str]) -> Option<String> {
    let root = repo_root();
    let script = root.join("scripts/vanilla-java.sh");
    if !script.exists() || !root.join("ref/bundle").exists() || Command::new("java").arg("-version").output().is_err() {
        eprintln!("SKIP: vanilla oracle (scripts/vanilla-java.sh, ref/bundle, java) unavailable");
        return None;
    }
    let out = Command::new(script).arg(fixtures().join(program)).args(args).output().expect("spawn vanilla-java.sh");
    assert!(out.status.success(), "{program} {args:?} failed:\n{}", String::from_utf8_lossy(&out.stderr));
    Some(String::from_utf8(out.stdout).unwrap())
}

pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
        0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
        0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&(data.len() as u64 * 8).to_be_bytes());
    for block in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

/// Mirrors `RegionTool.java dump`; only slots with `has=true` are listed.
pub fn dump_dir(dir: &Path) -> Vec<String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "mca"))
        .collect();
    files.sort();
    let mut lines = Vec::new();
    for p in files {
        let name = p.file_name().unwrap().to_str().unwrap().to_owned();
        let parts: Vec<&str> = name.split('.').collect();
        let (rx, rz): (i32, i32) = (parts[1].parse().unwrap(), parts[2].parse().unwrap());
        let f = RegionFile::new(info(), &p, dir, false).unwrap();
        for z in 0..32 {
            for x in 0..32 {
                let pos = ChunkPos::new(rx * 32 + x, rz * 32 + z);
                if !f.has_chunk(&pos) {
                    continue;
                }
                let exists = f.does_chunk_exist(&pos);
                let (len, sha) = match f.get_chunk_data_input_stream(&pos) {
                    Ok(Some(mut s)) => {
                        let mut b = Vec::new();
                        match s.read_to_end(&mut b) {
                            Ok(_) => (b.len().to_string(), sha256_hex(&b)),
                            Err(_) => ("ioerror".into(), "-".into()),
                        }
                    }
                    Ok(None) => ("null".into(), "-".into()),
                    Err(_) => ("ioerror".into(), "-".into()),
                };
                lines.push(format!("{name} {} {} has=true exists={exists} len={len} sha256={sha}", pos.x, pos.z));
            }
        }
    }
    lines
}

/// Vanilla `dump` output restricted to `has=true` slots.
pub fn java_dump(dir: &Path) -> Option<Vec<String>> {
    let out = run_java("RegionTool.java", &["dump", dir.to_str().unwrap()])?;
    Some(out.lines().filter(|l| l.contains(" has=true ")).map(str::to_owned).collect())
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

#[test]
fn sha256_vectors() {
    assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
}
