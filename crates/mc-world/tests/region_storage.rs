//! `RegionFileStorage` (region cache, NBT IO), `SimpleRegionStorage` (pending writes) and the
//! `EntityStorage` IO layer.

mod region_common;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_nbt::{CompoundTag, ListTag};
use mc_world::region::entity_storage::{ChunkEntities, PositionProblem};
use mc_world::region::region_file_version::VERSION_NONE;
use mc_world::region::{EntityStorage, RegionFile, RegionFileStorage, SimpleRegionStorage};
use region_common::*;

fn sample(n: i32) -> CompoundTag {
    let mut t = CompoundTag::new();
    t.put_int("DataVersion", 5023);
    t.put_string("name", format!("chunk {n}"));
    t.put_long_array("data", (0..300).map(|i| i64::from(i * n)).collect());
    t
}

#[test]
fn storage_read_write_clear() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("region"); // does not exist yet
    let mut s = RegionFileStorage::new(info(), &folder, false);
    let p = ChunkPos::new(-1, 33);
    assert_eq!(s.read(&p).unwrap(), None);
    assert!(!folder.exists(), "reading must not create files");
    s.write(&p, Some(&sample(1))).unwrap();
    assert!(folder.join("r.-1.1.mca").is_file());
    assert_eq!(s.read(&p).unwrap(), Some(sample(1)));
    assert_eq!(s.read(&ChunkPos::new(-2, 33)).unwrap(), None); // same region, other chunk
    s.write(&p, Some(&sample(2))).unwrap();
    assert_eq!(s.read(&p).unwrap(), Some(sample(2)));
    s.flush().unwrap();
    s.write(&p, None).unwrap();
    assert_eq!(s.read(&p).unwrap(), None);
    s.write(&p, Some(&sample(3))).unwrap();
    s.close().unwrap();
    let mut s = RegionFileStorage::new(info(), &folder, true);
    assert_eq!(s.read(&p).unwrap(), Some(sample(3)));
    // external dir files (.mcc) are placed in the storage folder
    let big = {
        let mut t = sample(4);
        t.put_byte_array("blob", (0..1_200_000u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as i8).collect());
        t
    };
    s.write(&ChunkPos::new(5, 33), Some(&big)).unwrap();
    assert_eq!(s.read(&ChunkPos::new(5, 33)).unwrap(), Some(big));
    s.close().unwrap();
}

#[test]
fn region_cache_is_lru_with_256_entries() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = RegionFileStorage::new(info(), dir.path(), false);
    for rx in 0..300 {
        s.write(&ChunkPos::new(rx * 32, 0), Some(&sample(rx))).unwrap();
    }
    let keys = s.cached_region_keys();
    assert_eq!(keys.len(), 256);
    assert_eq!(keys[0], ChunkPos::pack_xz(299, 0)); // most recently used first
    assert_eq!(*keys.last().unwrap(), ChunkPos::pack_xz(44, 0)); // regions 0..43 were evicted
    // touching moves to the front; negative lookups are cached too
    assert_eq!(s.read(&ChunkPos::new(44 * 32, 0)).unwrap(), Some(sample(44)));
    assert_eq!(s.cached_region_keys()[0], ChunkPos::pack_xz(44, 0));
    assert_eq!(s.cached_region_keys().last(), Some(&ChunkPos::pack_xz(45, 0)));
    assert_eq!(s.read(&ChunkPos::new(0, 5000)).unwrap(), None);
    assert_eq!(s.cached_region_keys()[0], ChunkPos::pack_xz(0, 156));
    assert_eq!(s.cached_region_keys().len(), 256);
    // evicted regions were closed properly and reopen with their data
    assert_eq!(s.read(&ChunkPos::new(0, 0)).unwrap(), Some(sample(0)));
    s.close().unwrap();
    assert!(s.cached_region_keys().is_empty());
}

#[test]
fn corrupt_chunk_nbt_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let mut f = RegionFile::with_version(info(), &dir.path().join("r.0.0.mca"), dir.path(), &VERSION_NONE, false).unwrap();
    f.write_chunk_data(&ChunkPos::new(0, 0), &[10, 0, 0, 1, 0, 1]).unwrap(); // truncated compound
    f.write_chunk_data(&ChunkPos::new(1, 0), &[9, 0, 0, 0]).unwrap(); // root is not a compound
    f.write_chunk_data(&ChunkPos::new(2, 0), &[]).unwrap(); // empty payload
    f.close().unwrap();
    let mut s = RegionFileStorage::new(info(), dir.path(), false);
    for x in 0..3 {
        assert!(s.read(&ChunkPos::new(x, 0)).is_err(), "{x}");
    }
}

#[test]
fn simple_region_storage_pending_semantics() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = SimpleRegionStorage::new(info(), dir.path(), false);
    let (a, b) = (ChunkPos::new(0, 0), ChunkPos::new(1, 0));
    s.write(&a, Some(sample(1)));
    s.write(&b, Some(sample(2)));
    s.write(&a, Some(sample(3))); // replaces the value, keeps the queue slot
    assert_eq!(s.read(&a).unwrap(), Some(sample(3)));
    assert!(!dir.path().join("r.0.0.mca").exists(), "nothing persisted before a drain");
    assert!(s.store_pending_chunk().unwrap()); // oldest first: `a`
    assert_eq!(s.region_file_storage().read(&a).unwrap(), Some(sample(3)));
    assert_eq!(s.region_file_storage().read(&b).unwrap(), None);
    s.write(&a, None); // pending removal shadows the stored chunk
    assert_eq!(s.read(&a).unwrap(), None);
    s.synchronize(true).unwrap();
    assert_eq!(s.read(&a).unwrap(), None);
    assert_eq!(s.read(&b).unwrap(), Some(sample(2)));
    s.write(&b, Some(sample(9)));
    s.close().unwrap(); // close drains
    let mut s = SimpleRegionStorage::new(info(), dir.path(), false);
    assert_eq!(s.read(&b).unwrap(), Some(sample(9)));
    assert_eq!(s.storage_info().r#type, "chunk");
}

fn entity(id: &str, x: f64) -> CompoundTag {
    let mut t = CompoundTag::new();
    t.put_string("id", id);
    let mut pos = ListTag::new();
    pos.push(x);
    pos.push(64.0);
    pos.push(0.5);
    t.put("Pos", pos);
    t
}

#[test]
fn entity_storage_layout_and_empty_chunks() {
    let dir = tempfile::tempdir().unwrap();
    let mut es = EntityStorage::new(SimpleRegionStorage::new(info(), dir.path(), false));
    let p = ChunkPos::new(-3, 7);

    let absent = es.load_entities(&p).unwrap();
    assert!(absent.chunk.is_empty() && absent.position_problem.is_none());

    let chunk = ChunkEntities::new(p, vec![entity("minecraft:cow", 1.5), entity("minecraft:pig", 2.5)]);
    es.store_entities(&chunk);
    es.flush(true).unwrap();
    let raw = es.simple_region_storage().region_file_storage().read(&p).unwrap().unwrap();
    let mut keys: Vec<&str> = raw.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["DataVersion", "Entities", "Position"]);
    assert_eq!(raw.get_int("DataVersion"), Some(5023));
    assert_eq!(raw.get_int_array("Position"), Some(&[-3, 7][..]));
    assert_eq!(raw.get_list("Entities").unwrap().len(), 2);
    let loaded = es.load_entities(&p).unwrap();
    assert_eq!(loaded.chunk, chunk);
    assert_eq!(loaded.position_problem, None);

    // storing an empty chunk removes the record; a second empty store is a no-op
    es.store_entities(&ChunkEntities::new(p, vec![]));
    es.flush(false).unwrap();
    assert_eq!(es.simple_region_storage().region_file_storage().read(&p).unwrap(), None);
    es.store_entities(&ChunkEntities::new(p, vec![]));
    assert!(es.load_entities(&p).unwrap().chunk.is_empty());

    // misplaced / unparseable position, non-compound entries skipped
    let mut tag = CompoundTag::new();
    tag.put_int("DataVersion", 5023);
    let mut list = ListTag::new();
    list.push(entity("minecraft:cow", 0.0));
    list.push(CompoundTag::new().with("", 5i32)); // wrapper compound: still a compound
    tag.put("Entities", list);
    tag.put_int_array("Position", vec![1, 1]);
    let q = ChunkPos::new(2, 2);
    es.simple_region_storage().write(&q, Some(tag.clone()));
    let l = es.load_entities(&q).unwrap();
    assert_eq!(l.position_problem, Some(PositionProblem::Misplaced { stored: ChunkPos::new(1, 1) }));
    assert_eq!(l.chunk.get_entities().len(), 2);
    let r = ChunkPos::new(4, 4);
    tag.put_string("Position", "x");
    es.simple_region_storage().write(&r, Some(tag.clone()));
    assert_eq!(es.load_entities(&r).unwrap().position_problem, Some(PositionProblem::Unparseable));
    let t = ChunkPos::new(5, 5);
    tag.remove("Position");
    tag.put("Entities", 7i32);
    es.simple_region_storage().write(&t, Some(tag));
    let l = es.load_entities(&t).unwrap();
    assert!(l.chunk.is_empty() && l.position_problem == Some(PositionProblem::Unparseable));
    es.close().unwrap();
}

#[test]
fn worldref_entities_load_and_resave() {
    let world = repo_root().join("run/servers/worldref/world/dimensions/minecraft");
    let Some(src) = ["overworld", "the_end"].map(|d| world.join(d).join("entities")).into_iter().find(|d| d.is_dir()) else {
        eprintln!("SKIP: worldref entities absent");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    copy_region_files(&src, &tmp.path().join("in"));
    let mut es = EntityStorage::new(SimpleRegionStorage::new(info(), tmp.path().join("in"), false));
    let out = tmp.path().join("out");
    let mut copy = EntityStorage::new(SimpleRegionStorage::new(info(), &out, false));
    let mut n = 0;
    let mut with_entities = 0;
    for line in dump_dir(&tmp.path().join("in")) {
        let f: Vec<&str> = line.split(' ').collect();
        let pos = ChunkPos::new(f[1].parse().unwrap(), f[2].parse().unwrap());
        let l = es.load_entities(&pos).unwrap();
        assert_eq!(l.position_problem, None, "{pos}");
        assert!(l.skipped_entries.is_empty());
        with_entities += usize::from(!l.chunk.is_empty());
        copy.store_entities(&l.chunk);
        n += 1;
    }
    copy.flush(true).unwrap();
    let mut back = EntityStorage::new(SimpleRegionStorage::new(info(), &out, false));
    let mut kept = 0;
    for line in dump_dir(&tmp.path().join("in")) {
        let f: Vec<&str> = line.split(' ').collect();
        let pos = ChunkPos::new(f[1].parse().unwrap(), f[2].parse().unwrap());
        let (a, b) = (es.load_entities(&pos).unwrap(), back.load_entities(&pos).unwrap());
        assert_eq!(a.chunk, b.chunk, "{pos}");
        kept += usize::from(!b.chunk.is_empty());
    }
    assert_eq!(kept, with_entities);
    eprintln!("entities: {n} chunks, {with_entities} non-empty");
}
