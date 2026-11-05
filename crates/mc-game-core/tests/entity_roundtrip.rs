//! Every entity of the vanilla world `run/servers/worldref` (entities region files of all dimensions) is loaded
//! through `EntityType::create`/`Entity.load` and saved again; the NBT must equal the original.

extern crate mc_game_core as mc_game;
use mc_core::ident::Identifier;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_game::server::level::ServerLevel;
use mc_game::world::entity::{entity as entity_mod, factory};
use mc_nbt::CompoundTag;
use mc_world::region::{EntityStorage, RegionStorageInfo, SimpleRegionStorage};
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn worldref_entities_round_trip() {
    let world = repo_root().join("run/servers/worldref/world/dimensions/minecraft");
    let mut checked = 0;
    let mut failures = Vec::new();
    let level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    for dim in ["overworld", "the_nether", "the_end"] {
        let dir = world.join(dim).join("entities");
        if !dir.is_dir() {
            continue;
        }
        let tmp = tempfile::tempdir().unwrap();
        for e in std::fs::read_dir(&dir).unwrap() {
            let e = e.unwrap();
            std::fs::copy(e.path(), tmp.path().join(e.file_name())).unwrap();
        }
        let mut storage = EntityStorage::new(SimpleRegionStorage::new(
            RegionStorageInfo::new("test", Identifier::parse(&format!("minecraft:{dim}")).unwrap(), "entities"),
            tmp.path(),
            false,
        ));
        for rx in -2..2 {
            for rz in -2..2 {
                if !tmp.path().join(format!("r.{rx}.{rz}.mca")).exists() {
                    continue;
                }
                for cx in 0..32 {
                    for cz in 0..32 {
                        let pos = ChunkPos::new(rx * 32 + cx, rz * 32 + cz);
                        let loaded = storage.load_entities(&pos).unwrap();
                        for tag in loaded.chunk.get_entities() {
                            let entities = factory::load_entities_recursive(std::slice::from_ref(tag), &level, mc_data::enums::EntitySpawnReason::Load);
                            assert!(!entities.is_empty(), "entity not instantiated: {tag:?}");
                            // flat list: root first, passengers after; map id -> entity for the lookup
                            let by_id: std::collections::HashMap<i32, &dyn entity_mod::Entity> = entities.iter().map(|e| (e.id(), e.as_ref())).collect();
                            let lookup = |id: i32| by_id.get(&id).copied();
                            let saved = entity_mod::save(entities[0].as_ref(), &lookup).expect("entity saves");
                            checked += 1;
                            if &saved != tag {
                                failures.push((dim, pos, diff(tag, &saved)));
                            }
                        }
                    }
                }
            }
        }
    }
    eprintln!("checked {checked} entities");
    assert!(failures.is_empty(), "{} of {checked} differ, first: {:?}", failures.len(), failures.iter().take(5).collect::<Vec<_>>());
}

fn diff(a: &CompoundTag, b: &CompoundTag) -> String {
    let mut out = Vec::new();
    for (k, v) in a.iter() {
        match b.get(k) {
            None => out.push(format!("missing {k}")),
            Some(w) if w != v => out.push(format!("{k}: {v:?} != {w:?}")),
            _ => {}
        }
    }
    for k in b.keys() {
        if a.get(k).is_none() {
            out.push(format!("extra {k}"));
        }
    }
    out.join("; ")
}
