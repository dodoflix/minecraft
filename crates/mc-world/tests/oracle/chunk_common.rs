//! Shared helpers for the chunk-level tests against the vanilla reference world.
#![allow(dead_code, reason = "each test target uses a subset of the shared helpers")]

use mc_core::ident::Identifier;
use mc_core::pos::ChunkPos;
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_world::chunk::LevelHeight;
use mc_world::palette::{BiomeRegistry, PalettedContainerFactory};
use mc_world::region::{RegionFileStorage, RegionStorageInfo};
use std::path::Path;

/// `(dimension path, height, has sky light)`.
pub const DIMENSIONS: [(&str, LevelHeight, bool); 3] = [
    ("overworld", LevelHeight::new(-64, 384), true),
    ("the_nether", LevelHeight::new(0, 256), false),
    ("the_end", LevelHeight::new(0, 256), true),
];

pub fn factory() -> PalettedContainerFactory {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/generated/data/minecraft/worldgen/biome");
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("ref/ is missing: run scripts/setup-ref.sh")
        .map(|e| format!("minecraft:{}", e.unwrap().file_name().to_str().unwrap().trim_end_matches(".json")))
        .collect();
    names.sort();
    PalettedContainerFactory::create(BiomeRegistry::new(names)).unwrap()
}

pub fn storage(folder: &Path, kind: &str) -> RegionFileStorage {
    RegionFileStorage::new(
        RegionStorageInfo::new("test", Identifier::parse("minecraft:overworld").unwrap(), kind),
        folder,
        false,
    )
}

/// Every chunk (position + NBT) of the `.mca` files in `folder`.
pub fn read_all(folder: &Path, kind: &str) -> Vec<(ChunkPos, CompoundTag)> {
    let mut regions: Vec<(i32, i32)> = Vec::new();
    for e in std::fs::read_dir(folder).into_iter().flatten() {
        let name = e.unwrap().file_name().into_string().unwrap();
        if let Some(rest) = name.strip_prefix("r.").and_then(|r| r.strip_suffix(".mca")) {
            let (x, z) = rest.split_once('.').unwrap();
            regions.push((x.parse().unwrap(), z.parse().unwrap()));
        }
    }
    regions.sort();
    let mut storage = storage(folder, kind);
    let mut out = Vec::new();
    for (rx, rz) in regions {
        for lz in 0..32 {
            for lx in 0..32 {
                let pos = ChunkPos::new(rx * 32 + lx, rz * 32 + lz);
                if let Some(tag) = storage.read(&pos).unwrap() {
                    out.push((pos, tag));
                }
            }
        }
    }
    out
}

/// Order-insensitive normal form of the parts of a chunk tag whose order is a Java hash-set artefact.
pub fn normalize(tag: &CompoundTag) -> CompoundTag {
    let mut t = tag.clone();
    if let Some(Tag::List(list)) = t.get_mut("block_entities") {
        let mut items: Vec<Tag> = list.iter().cloned().collect();
        items.sort_by_key(|b| {
            let c = b.as_compound().unwrap();
            (c.get_int_or("x", 0), c.get_int_or("y", 0), c.get_int_or("z", 0))
        });
        *list = ListTag::from(items);
    }
    if let Some(Tag::Compound(structures)) = t.get_mut("structures") {
        if let Some(Tag::Compound(refs)) = structures.get_mut("References") {
            for (_, v) in refs.iter_mut() {
                if let Tag::LongArray(l) = v {
                    l.sort_unstable();
                }
            }
        }
    }
    t
}

/// First difference between two tags, as a path (for readable failures).
pub fn diff(a: &Tag, b: &Tag, path: &str) -> Option<String> {
    match (a, b) {
        (Tag::Compound(x), Tag::Compound(y)) => {
            for (k, v) in x {
                match y.get(k) {
                    None => return Some(format!("{path}/{k}: missing in second")),
                    Some(w) => {
                        if let Some(d) = diff(v, w, &format!("{path}/{k}")) {
                            return Some(d);
                        }
                    }
                }
            }
            y.keys().find(|k| !x.contains(k)).map(|k| format!("{path}/{k}: missing in first"))
        }
        (Tag::List(x), Tag::List(y)) => {
            if x.len() != y.len() {
                return Some(format!("{path}: list length {} vs {}", x.len(), y.len()));
            }
            x.iter().zip(y.iter()).enumerate().find_map(|(i, (p, q))| diff(p, q, &format!("{path}[{i}]")))
        }
        _ if a == b => None,
        _ => Some(format!("{path}: {a:?} vs {b:?}").chars().take(400).collect()),
    }
}
