//! `RegionFile` differential checks against the vanilla Java `RegionFile` (`fixtures/region/RegionTool.java`, run
//! through `scripts/vanilla-java.sh`): the same malformed files, files we write, and oversized chunks vanilla writes.

use crate::region_common::*;
use crate::region_oracle::*;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_world::region::RegionFile;
use mc_world::region::region_file_version::{self as v, RegionFileVersion};
use std::path::Path;

fn pseudo_random(n: usize, mut seed: u64) -> Vec<u8> {
    (0..n)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 24) as u8
        })
        .collect()
}

fn open(dir: &Path, name: &str, version: &'static RegionFileVersion) -> RegionFile {
    RegionFile::with_version(info(), &dir.join(name), dir, version, false).unwrap()
}

/// Our dump of `dir` must equal what the vanilla RegionFile reports.
fn assert_matches_vanilla(dir: &Path, what: &str) {
    assert_eq!(java_dump(dir), dump_dir(dir), "{what}");
}

#[test]
#[ignore = "oracle: runs scripts/vanilla-java.sh"]
fn malformed_files_match_vanilla() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("r.0.0.mca");
    std::fs::write(&path, invalid_header_file()).unwrap();
    assert_matches_vanilla(dir.path(), "invalid header entries");
    for (name, length, version, _) in STREAM_CASES {
        std::fs::write(&path, stream_case_file(length, version)).unwrap();
        assert_matches_vanilla(dir.path(), name);
    }
    std::fs::write(&path, corrupt_deflate_file()).unwrap();
    assert_matches_vanilla(dir.path(), "corrupt deflate data");
}

#[test]
#[ignore = "oracle: runs scripts/vanilla-java.sh"]
fn java_reads_files_written_by_us() {
    let tmp = tempfile::tempdir().unwrap();
    for (n, ver) in [
        &v::VERSION_GZIP,
        &v::VERSION_DEFLATE,
        &v::VERSION_NONE,
        &v::VERSION_LZ4,
    ]
    .into_iter()
    .enumerate()
    {
        let rx = ver.get_id();
        let mut f = open(tmp.path(), &format!("r.{rx}.0.mca"), ver);
        for i in 0..40 {
            let size = [0, 1, 4090, 4091, 4092, 8187, 20_000, 70_000, 200_000][(i + n) % 9];
            let data = if i % 2 == 0 {
                pseudo_random(size, i as u64 + 1)
            } else {
                vec![(i % 5) as u8; size]
            };
            f.write_chunk_data(
                &ChunkPos::new(rx * 32 + (i % 32) as i32, (i / 32) as i32),
                &data,
            )
            .unwrap();
        }
        // overwrite a few (frees + reuses sectors) and drop one
        f.write_chunk_data(&ChunkPos::new(rx * 32 + 3, 0), &pseudo_random(30_000, 99))
            .unwrap();
        f.clear(&ChunkPos::new(rx * 32 + 4, 0)).unwrap();
        if ver.get_id() >= 3 {
            f.write_chunk_data(
                &ChunkPos::new(rx * 32 + 6, 7),
                &pseudo_random(1_200_000, 77),
            )
            .unwrap(); // oversized -> .mcc
        }
        f.close().unwrap();
    }
    let ours = dump_dir(tmp.path());
    assert!(
        ours.len() > 150
            && ours
                .iter()
                .all(|l| !l.contains("len=null") && !l.contains("ioerror")),
        "{ours:?}"
    );
    assert_eq!(java_dump(tmp.path()), ours);
    assert!(tmp.path().join("c.102.7.mcc").exists() && tmp.path().join("c.134.7.mcc").exists());
}

#[test]
#[ignore = "oracle: runs scripts/vanilla-java.sh"]
fn we_read_files_with_oversized_chunks_written_by_java() {
    let tmp = tempfile::tempdir().unwrap();
    run_java("RegionTool.java", &["genbig", tmp.path().to_str().unwrap()]);
    assert!(
        std::fs::read_dir(tmp.path()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_str()
            .unwrap()
            .ends_with(".mcc"))
    );
    let ours = dump_dir(tmp.path());
    assert_eq!(ours.len(), 6);
    assert_eq!(java_dump(tmp.path()), ours);
    assert!(ours.iter().all(|l| !l.contains("len=null")));
}
