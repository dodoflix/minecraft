//! `RegionFile`: sector allocation, header validation, corrupt-file handling, oversized external
//! chunks, every compression version, and differential checks against the vanilla Java `RegionFile`.

mod region_common;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_world::region::region_file_version::{self as v, RegionFileVersion};
use mc_world::region::RegionFile;
use region_common::*;
use std::io::Read;
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

fn read(f: &RegionFile, x: i32, z: i32) -> Option<Vec<u8>> {
    let mut b = Vec::new();
    f.get_chunk_data_input_stream(&ChunkPos::new(x, z)).unwrap()?.read_to_end(&mut b).unwrap();
    Some(b)
}

fn open(dir: &Path, name: &str, version: &'static RegionFileVersion) -> RegionFile {
    RegionFile::with_version(info(), &dir.join(name), dir, version, false).unwrap()
}

fn sector(offset: i32) -> (usize, usize) {
    ((offset >> 8 & 0xFFFFFF) as usize, (offset & 0xFF) as usize)
}

#[test]
fn sector_allocation_reuse_and_padding() {
    let dir = tempfile::tempdir().unwrap();
    let mut f = open(dir.path(), "r.0.0.mca", &v::VERSION_NONE);
    let (a, b, c) = (ChunkPos::new(0, 0), ChunkPos::new(1, 0), ChunkPos::new(0, 1));
    f.write_chunk_data(&a, &[1; 100]).unwrap();
    assert_eq!(sector(f.get_offset(&a)), (2, 1));
    f.write_chunk_data(&b, &pseudo_random(9000, 1)).unwrap(); // 5 + 9000 -> 3 sectors
    assert_eq!(sector(f.get_offset(&b)), (3, 3));
    f.write_chunk_data(&a, &pseudo_random(4091, 2)).unwrap(); // 5 + 4091 = 4096 -> still 1 sector, but the old one is freed only after
    assert_eq!(sector(f.get_offset(&a)), (6, 1));
    f.write_chunk_data(&c, &[7; 4092]).unwrap(); // 4097 bytes -> 2 sectors; first fit is the single freed sector? no: 2 needed
    assert_eq!(sector(f.get_offset(&c)), (7, 2));
    f.write_chunk_data(&ChunkPos::new(2, 0), &[9; 10]).unwrap();
    assert_eq!(sector(f.get_offset(&ChunkPos::new(2, 0))), (2, 1)); // reuses the sector freed by rewriting `a`
    assert_eq!(f.used_sectors().get_used(), (0..9).collect::<Vec<_>>());
    f.close().unwrap();
    let len = std::fs::metadata(dir.path().join("r.0.0.mca")).unwrap().len();
    assert_eq!(len, 9 * 4096);

    let f = open(dir.path(), "r.0.0.mca", &v::VERSION_NONE);
    assert_eq!(read(&f, 0, 0).unwrap(), pseudo_random(4091, 2));
    assert_eq!(read(&f, 1, 0).unwrap(), pseudo_random(9000, 1));
    assert_eq!(read(&f, 0, 1).unwrap(), vec![7; 4092]);
    assert_eq!(f.used_sectors().get_used(), (0..9).collect::<Vec<_>>());
    assert!(f.get_timestamp(&a) > 1_700_000_000);
    assert!(read(&f, 9, 9).is_none() && !f.has_chunk(&ChunkPos::new(9, 9)) && !f.does_chunk_exist(&ChunkPos::new(9, 9)));
}

#[test]
fn untouched_new_file_stays_empty_and_clear_frees() {
    let dir = tempfile::tempdir().unwrap();
    let f = open(dir.path(), "r.0.0.mca", &v::VERSION_DEFLATE);
    f.close().unwrap();
    assert_eq!(std::fs::metadata(dir.path().join("r.0.0.mca")).unwrap().len(), 0);

    let mut f = open(dir.path(), "r.0.0.mca", &v::VERSION_DEFLATE);
    let p = ChunkPos::new(3, 4);
    f.clear(&p).unwrap(); // absent: no-op
    f.write_chunk_data(&p, b"payload").unwrap();
    assert!(f.has_chunk(&p) && f.does_chunk_exist(&p));
    f.clear(&p).unwrap();
    assert!(!f.has_chunk(&p) && read(&f, 3, 4).is_none());
    assert_eq!(f.used_sectors().get_used(), vec![0, 1]);
    f.close().unwrap();
    // header persisted
    let f = open(dir.path(), "r.0.0.mca", &v::VERSION_DEFLATE);
    assert!(!f.has_chunk(&p));
}

#[test]
fn every_version_round_trips_and_ids_are_stored() {
    let dir = tempfile::tempdir().unwrap();
    let data = pseudo_random(40_000, 5).into_iter().enumerate().map(|(i, b)| if i % 3 == 0 { 0 } else { b }).collect::<Vec<_>>();
    for ver in [&v::VERSION_GZIP, &v::VERSION_DEFLATE, &v::VERSION_NONE, &v::VERSION_LZ4] {
        let name = format!("r.{}.0.mca", ver.get_id());
        let mut f = open(dir.path(), &name, ver);
        f.write_chunk_data(&ChunkPos::new(ver.get_id() * 32 + 4, 5), &data).unwrap();
        f.close().unwrap();
        let raw = std::fs::read(dir.path().join(&name)).unwrap();
        let (n, _) = sector(i32::from_be_bytes(raw[(4 + 5 * 32) * 4..(4 + 5 * 32) * 4 + 4].try_into().unwrap()));
        assert_eq!(raw[n * 4096 + 4], ver.get_id() as u8);
        let len = i32::from_be_bytes(raw[n * 4096..n * 4096 + 4].try_into().unwrap()) as usize;
        let payload = &raw[n * 4096 + 5..n * 4096 + 4 + len];
        if ver.get_id() == 3 {
            assert_eq!(payload, &data[..]);
        }
        let f = open(dir.path(), &name, &v::VERSION_DEFLATE); // reading is independent of the write version
        assert_eq!(read(&f, ver.get_id() * 32 + 4, 5).unwrap(), data, "version {}", ver.get_id());
    }
    // writing with the custom version fails like vanilla's UnsupportedOperationException
    let mut f = open(dir.path(), "r.9.0.mca", &v::VERSION_CUSTOM);
    assert!(f.write_chunk_data(&ChunkPos::new(0, 0), b"x").is_err());
}

#[test]
fn oversized_chunk_goes_to_mcc() {
    let dir = tempfile::tempdir().unwrap();
    let mut f = open(dir.path(), "r.0.0.mca", &v::VERSION_NONE);
    let p = ChunkPos::new(2, 3);
    let big = pseudo_random(1_100_000, 3);
    f.write_chunk_data(&p, &big).unwrap();
    let mcc = dir.path().join("c.2.3.mcc");
    assert_eq!(std::fs::read(&mcc).unwrap(), big);
    assert_eq!(sector(f.get_offset(&p)), (2, 1));
    assert_eq!(read(&f, 2, 3).unwrap(), big);
    assert!(f.does_chunk_exist(&p));
    f.close().unwrap();
    let raw = std::fs::read(dir.path().join("r.0.0.mca")).unwrap();
    assert_eq!(&raw[8192..8197], &[0, 0, 0, 1, 3 | 128]);
    assert_eq!(raw.len(), 3 * 4096);

    // 255 sectors minus the 5-byte header still fits inline
    let mut f = open(dir.path(), "r.0.0.mca", &v::VERSION_NONE);
    let q = ChunkPos::new(4, 4);
    f.write_chunk_data(&q, &pseudo_random(255 * 4096 - 5, 4)).unwrap();
    assert_eq!(sector(f.get_offset(&q)).1, 255);
    assert!(!dir.path().join("c.4.4.mcc").exists());
    f.write_chunk_data(&q, &pseudo_random(255 * 4096 - 4, 4)).unwrap();
    assert!(dir.path().join("c.4.4.mcc").exists());

    // rewriting small removes the external file; clearing removes it too
    f.write_chunk_data(&p, b"small").unwrap();
    assert!(!mcc.exists());
    f.clear(&q).unwrap();
    assert!(!dir.path().join("c.4.4.mcc").exists());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_str().unwrap().starts_with("tmp")).count(), 0);
}

/// Our dump of `dir` must equal what the vanilla RegionFile reports.
fn assert_matches_vanilla(dir: &Path, what: &str) {
    if let Some(java) = java_dump(dir) {
        assert_eq!(java, dump_dir(dir), "{what}");
    }
}

fn put_i32(buf: &mut [u8], at: usize, v: i32) {
    buf[at..at + 4].copy_from_slice(&v.to_be_bytes());
}

#[test]
fn malformed_files_behave_like_vanilla() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("r.0.0.mca");

    // truncated header: opens, nothing readable
    std::fs::write(&path, vec![0u8; 100]).unwrap();
    let f = open(dir.path(), "r.0.0.mca", &v::VERSION_DEFLATE);
    assert!(!f.has_chunk(&ChunkPos::new(0, 0)));
    f.close().unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 4096); // close pads to a sector

    // invalid header entries are zeroed: slots 0..3 = overlaps header, zero sectors, out of bounds
    let mut file = vec![0u8; 3 * 4096];
    put_i32(&mut file, 0, 1 << 8 | 1);
    put_i32(&mut file, 4, 2 << 8);
    put_i32(&mut file, 8, 100 << 8 | 1);
    put_i32(&mut file, 12, 2 << 8 | 1); // valid but length 0
    put_i32(&mut file, 16, 3 << 8 | 1); // sector at EOF: header truncated
    put_i32(&mut file, 20, 2 << 8 | 1);
    std::fs::write(&path, &file).unwrap();
    assert_matches_vanilla(dir.path(), "invalid header entries");
    let f = open(dir.path(), "r.0.0.mca", &v::VERSION_DEFLATE);
    for x in 0..3 {
        assert!(!f.has_chunk(&ChunkPos::new(x, 0)), "{x}");
    }
    assert!(f.has_chunk(&ChunkPos::new(3, 0)));
    assert!(read(&f, 3, 0).is_none()); // allocated, stream missing
    assert!(!f.does_chunk_exist(&ChunkPos::new(3, 0)));
    assert!(read(&f, 4, 0).is_none()); // sector 3 is at EOF (3 * 4096 == size)
    assert!(!f.does_chunk_exist(&ChunkPos::new(4, 0)));
    assert_eq!(f.used_sectors().get_used(), vec![0, 1, 2, 3]);
    drop(f);

    // stream-level problems: each in its own sector-2 slot of a fresh file
    let cases: [(&str, i32, u8, bool); 6] = [
        ("negative length", -5, 2, false),
        ("stream longer than the sector", 5000, 2, false),
        ("unknown version byte", 10, 9, false),
        ("version byte >= 128 without external data", 10, 0x7E | 0x80, false),
        ("custom compression", 10, 127, true),
        ("external stream missing", 1, 2 | 128, false),
    ];
    for (name, length, version, exists_expected) in cases {
        let mut file = vec![0u8; 3 * 4096];
        put_i32(&mut file, 0, 2 << 8 | 1);
        put_i32(&mut file, 8192, length);
        file[8196] = version;
        file[8197..8199].copy_from_slice(&[0, 0]);
        std::fs::write(&path, &file).unwrap();
        assert_matches_vanilla(dir.path(), name);
        let f = open(dir.path(), "r.0.0.mca", &v::VERSION_DEFLATE);
        assert!(read(&f, 0, 0).is_none(), "{name}");
        // doesChunkExist only checks id validity, external file presence and declared length
        assert_eq!(f.does_chunk_exist(&ChunkPos::new(0, 0)), exists_expected, "{name}");
        drop(f);
    }

    // corrupt compressed data surfaces as an IO error while reading the stream
    let mut file = vec![0u8; 3 * 4096];
    put_i32(&mut file, 0, 2 << 8 | 1);
    put_i32(&mut file, 8192, 9);
    file[8196] = 2;
    file[8197..8205].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef, 1, 2, 3, 4]);
    std::fs::write(&path, &file).unwrap();
    assert_matches_vanilla(dir.path(), "corrupt deflate data");
    let f = open(dir.path(), "r.0.0.mca", &v::VERSION_DEFLATE);
    let mut s = f.get_chunk_data_input_stream(&ChunkPos::new(0, 0)).unwrap().unwrap();
    assert!(s.read_to_end(&mut Vec::new()).is_err());
    assert!(f.does_chunk_exist(&ChunkPos::new(0, 0)));
    drop((s, f));

    // external dir must be a directory
    assert!(RegionFile::new(info(), &path, &dir.path().join("nope"), false).is_err());
}

#[test]
fn sync_option_opens_with_dsync() {
    let dir = tempfile::tempdir().unwrap();
    let mut f = RegionFile::with_version(info(), &dir.path().join("r.0.0.mca"), dir.path(), &v::VERSION_LZ4, true).unwrap();
    f.write_chunk_data(&ChunkPos::new(0, 0), b"durable").unwrap();
    assert_eq!(read(&f, 0, 0).unwrap(), b"durable");
    f.flush().unwrap();
}

#[test]
fn reads_java_written_files() {
    let dir = fixtures().join("java");
    let expected: Vec<String> = std::fs::read_to_string(dir.join("expected.txt")).unwrap().lines().map(str::to_owned).collect();
    assert_eq!(dump_dir(&dir), expected);
    assert_eq!(expected.len(), 16);
}

#[test]
fn java_reads_files_written_by_us() {
    let tmp = tempfile::tempdir().unwrap();
    for (n, ver) in [&v::VERSION_GZIP, &v::VERSION_DEFLATE, &v::VERSION_NONE, &v::VERSION_LZ4].into_iter().enumerate() {
        let rx = ver.get_id();
        let mut f = open(tmp.path(), &format!("r.{rx}.0.mca"), ver);
        for i in 0..40 {
            let size = [0, 1, 4090, 4091, 4092, 8187, 20_000, 70_000, 200_000][(i + n) % 9];
            let data = if i % 2 == 0 { pseudo_random(size, i as u64 + 1) } else { vec![(i % 5) as u8; size] };
            f.write_chunk_data(&ChunkPos::new(rx * 32 + (i % 32) as i32, (i / 32) as i32), &data).unwrap();
        }
        // overwrite a few (frees + reuses sectors) and drop one
        f.write_chunk_data(&ChunkPos::new(rx * 32 + 3, 0), &pseudo_random(30_000, 99)).unwrap();
        f.clear(&ChunkPos::new(rx * 32 + 4, 0)).unwrap();
        if ver.get_id() >= 3 {
            f.write_chunk_data(&ChunkPos::new(rx * 32 + 6, 7), &pseudo_random(1_200_000, 77)).unwrap(); // oversized -> .mcc
        }
        f.close().unwrap();
    }
    let ours = dump_dir(tmp.path());
    assert!(ours.len() > 150 && ours.iter().all(|l| !l.contains("len=null") && !l.contains("ioerror")), "{ours:?}");
    if let Some(java) = java_dump(tmp.path()) {
        assert_eq!(java, ours);
    }
    assert!(tmp.path().join("c.102.7.mcc").exists() && tmp.path().join("c.134.7.mcc").exists());
}

#[test]
fn we_read_files_with_oversized_chunks_written_by_java() {
    let tmp = tempfile::tempdir().unwrap();
    let Some(_) = run_java("RegionTool.java", &["genbig", tmp.path().to_str().unwrap()]) else { return };
    assert!(std::fs::read_dir(tmp.path()).unwrap().any(|e| e.unwrap().file_name().to_str().unwrap().ends_with(".mcc")));
    let ours = dump_dir(tmp.path());
    assert_eq!(ours.len(), 6);
    assert_eq!(java_dump(tmp.path()).unwrap(), ours);
    assert!(ours.iter().all(|l| !l.contains("len=null")));
}
