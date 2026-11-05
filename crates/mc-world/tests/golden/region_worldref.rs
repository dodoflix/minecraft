//! Differential test over every region file of the vanilla-generated world `run/servers/worldref`
//! (overworld / nether / end, `region/`, `entities/`, `poi/`): our reads must equal the vanilla Java
//! `RegionFile` reads (chunk set + SHA-256 of every decompressed payload), files we rewrite with each
//! compression version must read back identically through vanilla, and vanilla-written lz4 files must
//! read back through us. Skips (passes) when the world or the Java oracle is absent.

use mc_core::pos::chunk_pos::ChunkPos;
use mc_world::region::region_file_version::{self as v, RegionFileVersion};
use mc_world::region::{RegionFile, RegionFileStorage};
use crate::region_common::*;
use std::path::{Path, PathBuf};

fn region_dirs(world: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let Ok(dims) = std::fs::read_dir(world.join("dimensions/minecraft")) else { return dirs };
    for dim in dims {
        for kind in ["region", "entities", "poi"] {
            let d = dim.as_ref().unwrap().path().join(kind);
            if d.is_dir() {
                dirs.push(d);
            }
        }
    }
    dirs.sort();
    dirs
}

/// Re-writes every chunk of `src` into `dst` with `version` using our RegionFile.
fn rewrite(src: &Path, dst: &Path, version: &'static RegionFileVersion) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_none_or(|x| x != "mca") {
            continue;
        }
        let name = p.file_name().unwrap().to_str().unwrap().to_owned();
        let parts: Vec<&str> = name.split('.').collect();
        let (rx, rz): (i32, i32) = (parts[1].parse().unwrap(), parts[2].parse().unwrap());
        let input = RegionFile::new(info(), &p, src, false).unwrap();
        let mut out = RegionFile::with_version(info(), &dst.join(&name), dst, version, false).unwrap();
        for z in 0..32 {
            for x in 0..32 {
                let pos = ChunkPos::new(rx * 32 + x, rz * 32 + z);
                if let Some(mut s) = input.get_chunk_data_input_stream(&pos).unwrap() {
                    let mut raw = Vec::new();
                    std::io::Read::read_to_end(&mut s, &mut raw).unwrap();
                    out.write_chunk_data(&pos, &raw).unwrap();
                }
            }
        }
        out.close().unwrap();
    }
}

#[test]
fn worldref_matches_vanilla_region_file() {
    let world = repo_root().join("run/servers/worldref/world");
    let dirs = region_dirs(&world);
    if dirs.is_empty() {
        eprintln!("SKIP: run/servers/worldref/world not populated");
        return;
    }
    let work = tempfile::tempdir().unwrap();
    let (mut total_chunks, mut total_files) = (0, 0);
    for (n, dir) in dirs.iter().enumerate() {
        let label = dir.strip_prefix(&world).unwrap().display().to_string();
        let copy = work.path().join(format!("src{n}"));
        copy_region_files(dir, &copy);

        // (1) every chunk of every file: ours == vanilla
        let ours = dump_dir(&copy);
        assert!(!ours.is_empty(), "{label}: no chunks");
        assert!(ours.iter().all(|l| l.contains(" exists=true ") && !l.contains("len=null") && !l.contains("ioerror")), "{label}: unreadable chunk");
        let Some(java) = java_dump(&copy) else { return };
        assert_eq!(java.len(), ours.len(), "{label}: chunk count");
        assert_eq!(java, ours, "{label}: payload hashes");
        total_chunks += ours.len();
        total_files += std::fs::read_dir(&copy).unwrap().count();

        // chunk NBT must parse fully through the storage layer
        let mut storage = RegionFileStorage::new(info(), &copy, false);
        for l in &ours {
            let f: Vec<&str> = l.split(' ').collect();
            let pos = ChunkPos::new(f[1].parse().unwrap(), f[2].parse().unwrap());
            let tag = storage.read(&pos).unwrap().unwrap_or_else(|| panic!("{label} {pos}: read returned none"));
            assert_eq!(tag.get_int("DataVersion"), Some(5023), "{label} {pos}");
        }
        storage.close().unwrap();

        // (2) our rewrite with every compression version is read identically by vanilla
        for ver in [&v::VERSION_DEFLATE, &v::VERSION_GZIP, &v::VERSION_NONE, &v::VERSION_LZ4] {
            let dst = work.path().join(format!("dst{n}_{}", ver.get_id()));
            rewrite(&copy, &dst, ver);
            assert_eq!(java_dump(&dst).unwrap(), java, "{label}: vanilla reading our version {} rewrite", ver.get_id());
        }

        // vanilla writes lz4 files, we read them
        let jdst = work.path().join(format!("jlz4_{n}"));
        run_java("RegionTool.java", &["write", copy.to_str().unwrap(), jdst.to_str().unwrap(), "4"]).unwrap();
        assert_eq!(dump_dir(&jdst), ours, "{label}: our reading of vanilla lz4 rewrite");
    }
    eprintln!("verified {total_chunks} chunks in {total_files} region files across {} folders", dirs.len());
}
