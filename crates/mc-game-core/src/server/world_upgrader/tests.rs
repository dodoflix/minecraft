use super::*;
use mc_core::ident::Identifier;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_datafix::CURRENT_VERSION;
use mc_nbt::CompoundTag;
use mc_world::region::{RegionFile, RegionFileStorage, RegionStorageInfo};
use std::path::Path;
use std::time::{Duration, Instant};

fn overworld() -> Identifier {
    Identifier::parse("minecraft:overworld").unwrap()
}

fn write_chunks(folder: &Path, version: Option<i32>, positions: &[(i32, i32)]) {
    let info = RegionStorageInfo::new("world", overworld(), "chunk");
    let mut storage = RegionFileStorage::new(info, folder, false);
    for &(x, z) in positions {
        let mut tag = CompoundTag::new();
        tag.put_int("xPos", x);
        tag.put_int("zPos", z);
        tag.put_string("Status", "minecraft:full");
        tag.put_byte("isLightOn", 1);
        if let Some(version) = version {
            tag.put_int("DataVersion", version);
        }
        storage
            .write(&ChunkPos::new(x, z), Some(&tag))
            .expect("write the chunk");
    }
    storage.close().expect("close the storage");
}

fn read_chunk(folder: &Path, x: i32, z: i32) -> Option<CompoundTag> {
    let info = RegionStorageInfo::new("world", overworld(), "chunk");
    let mut storage = RegionFileStorage::new(info, folder, false);
    let tag = storage.read(&ChunkPos::new(x, z)).expect("read the chunk");
    storage.close().expect("close the storage");
    tag
}

/// Runs the upgrader over a world whose overworld folder is `dir` and returns once it is finished.
fn run(dir: &Path, erase_cache: bool, recreate: bool) -> WorldUpgrader {
    let upgrader = WorldUpgrader::new(
        "world".to_owned(),
        vec![UpgradeDimension {
            key: overworld(),
            dir: dir.to_path_buf(),
            generator: Some(Identifier::parse("minecraft:noise").unwrap()),
        }],
        erase_cache,
        recreate,
    );
    let deadline = Instant::now() + Duration::from_secs(60);
    while !upgrader.is_finished() {
        assert!(Instant::now() < deadline, "the upgrade did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
    upgrader
}

fn old_version() -> i32 {
    // inside the chain, below the current version
    5000
}

#[test]
fn every_chunk_of_a_region_folder_is_upgraded_and_counted() {
    let dir = tempfile::tempdir().unwrap();
    let region = dir.path().join("region");
    write_chunks(&region, Some(old_version()), &[(0, 0), (1, 0), (-1, 5)]);
    let upgrader = run(dir.path(), false, false);
    assert_eq!(upgrader.get_total_chunks(), 3);
    assert_eq!(upgrader.get_converted() + upgrader.get_skipped(), 3);
    for (x, z) in [(0, 0), (1, 0), (-1, 5)] {
        let tag = read_chunk(&region, x, z).expect("chunk");
        assert_eq!(tag.get_int("DataVersion"), Some(CURRENT_VERSION));
        // without `--eraseCache` the light flag stays
        assert!(tag.contains("isLightOn"));
    }
    assert_eq!(upgrader.get_status(), "optimizeWorld.stage.finished.chunks");
}

#[test]
fn a_current_chunk_is_skipped_and_ends_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let region = dir.path().join("region");
    write_chunks(&region, Some(CURRENT_VERSION), &[(0, 0), (1, 0)]);
    let upgrader = run(dir.path(), false, false);
    // the first skipped chunk stops `converted && ..` from processing the rest of the file
    assert_eq!(upgrader.get_total_chunks(), 2);
    assert_eq!(upgrader.get_converted(), 0);
    assert_eq!(upgrader.get_skipped(), 1);
}

#[test]
fn erase_cache_rewrites_a_current_chunk_without_its_light_flag() {
    let dir = tempfile::tempdir().unwrap();
    let region = dir.path().join("region");
    write_chunks(&region, Some(CURRENT_VERSION), &[(0, 0)]);
    let upgrader = run(dir.path(), true, false);
    assert_eq!(upgrader.get_converted(), 1);
    assert!(!read_chunk(&region, 0, 0).unwrap().contains("isLightOn"));
}

#[test]
fn recreated_region_files_replace_the_originals_and_leave_no_new_folder() {
    let dir = tempfile::tempdir().unwrap();
    let region = dir.path().join("region");
    write_chunks(&region, Some(old_version()), &[(0, 0), (1, 0)]);
    let upgrader = run(dir.path(), false, true);
    assert_eq!(upgrader.get_converted(), 2);
    assert!(!dir.path().join("new_region").exists());
    let tag = read_chunk(&region, 1, 0).expect("chunk");
    assert_eq!(tag.get_int("DataVersion"), Some(CURRENT_VERSION));
}

#[test]
fn entity_and_poi_folders_are_upgraded_too() {
    let dir = tempfile::tempdir().unwrap();
    for folder in ["entities", "poi"] {
        write_chunks(&dir.path().join(folder), Some(old_version()), &[(2, 3)]);
    }
    let upgrader = run(dir.path(), false, false);
    assert_eq!(upgrader.get_status(), "optimizeWorld.stage.finished.chunks");
    for folder in ["entities", "poi"] {
        let tag = read_chunk(&dir.path().join(folder), 2, 3).unwrap();
        assert_eq!(
            tag.get_int("DataVersion"),
            Some(CURRENT_VERSION),
            "{folder}"
        );
    }
}

/// Writes the chunk at `pos` as bytes that do not parse (a compound whose first child, a byte named `a`, has no value).
fn write_unparseable_chunk(region: &Path, pos: (i32, i32)) {
    std::fs::create_dir_all(region).unwrap();
    let info = RegionStorageInfo::new("world", overworld(), "chunk");
    let mut file = RegionFile::new(info, &region.join("r.0.0.mca"), region, false).unwrap();
    file.write_chunk_data(&ChunkPos::new(pos.0, pos.1), &[10, 0, 0, 1, 0, 1, b'a'])
        .unwrap();
    file.close().unwrap();
}

#[test]
fn a_chunk_that_does_not_parse_fails_the_upgrade() {
    let dir = tempfile::tempdir().unwrap();
    write_unparseable_chunk(&dir.path().join("region"), (0, 0));
    let upgrader = run(dir.path(), false, false);
    assert_eq!(upgrader.get_status(), "optimizeWorld.stage.failed");
}

#[test]
fn the_write_queued_before_a_failure_is_still_stored() {
    let dir = tempfile::tempdir().unwrap();
    let region = dir.path().join("region");
    write_chunks(&region, Some(old_version()), &[(0, 0)]);
    write_unparseable_chunk(&region, (0, 1));
    let upgrader = run(dir.path(), false, false);
    assert_eq!(upgrader.get_status(), "optimizeWorld.stage.failed");
    let tag = read_chunk(&region, 0, 0).expect("chunk");
    assert_eq!(tag.get_int("DataVersion"), Some(CURRENT_VERSION));
}
