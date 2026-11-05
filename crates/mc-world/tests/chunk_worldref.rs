//! Differential test: every chunk of the vanilla-generated world `run/servers/worldref` must parse, load into the
//! chunk data model and serialize back to the same NBT (hash-set orderings aside). Skips when the world is absent.

mod chunk_common;

use chunk_common::*;
use mc_nbt::Tag;
use mc_world::data_layer::DataLayer;
use mc_world::serial::{LoadedChunk, ReadContext, SerializableChunkData};
use std::collections::HashMap;

#[test]
fn chunks_roundtrip_through_the_data_model() {
    let world = world_dir();
    if !world.exists() {
        eprintln!("SKIP: {} absent", world.display());
        return;
    }
    let factory = factory();
    let all_known = |_: &str| true;
    let mut chunks = 0;
    let mut by_type: HashMap<&str, usize> = HashMap::new();
    for (dim, height, has_sky) in DIMENSIONS {
        let folder = world.join(format!("dimensions/minecraft/{dim}/region"));
        for (pos, tag) in read_all(&folder, "chunk") {
            let data = SerializableChunkData::parse(height, &factory, &tag).unwrap().expect("status present");
            let game_time = data.last_update_time;
            let ctx = ReadContext { level_height: height, has_sky_light: has_sky, factory: &factory, is_known_structure: &all_known };
            let loaded = data.read(&ctx, pos);
            let light: HashMap<i32, (Option<DataLayer>, Option<DataLayer>)> =
                loaded.light.iter().map(|(y, b, s)| (*y, (b.clone(), s.clone()))).collect();
            let light_fn = |y: i32| light.get(&y).cloned().unwrap_or((None, None));
            let rewritten = match &loaded.chunk {
                LoadedChunk::Proto(p) => {
                    *by_type.entry("proto").or_default() += 1;
                    let be: Vec<_> = p.access.block_entities.values().cloned().collect();
                    SerializableChunkData::from_proto_chunk(p, game_time, &light_fn, be)
                }
                LoadedChunk::Level(l) => {
                    *by_type.entry("level").or_default() += 1;
                    SerializableChunkData::from_level_chunk(l, game_time, &light_fn, loaded.post_load_block_entities.clone())
                }
            };
            let mut out = rewritten.write(&factory).unwrap();
            // like vanilla, loading primes the heightmaps a status requires but the file lacks
            if let LoadedChunk::Proto(p) = &loaded.chunk {
                let hm = out.get_compound_mut("Heightmaps").unwrap();
                for ty in p.get_persisted_status().heightmaps_after() {
                    if tag.get_compound("Heightmaps").is_none_or(|h| !h.contains(ty.serialization_key())) {
                        hm.remove(ty.serialization_key());
                    }
                }
            }
            if let Some(d) = diff(&Tag::Compound(normalize(&tag)), &Tag::Compound(normalize(&out)), "") {
                panic!("{dim} chunk {pos}: {d}");
            }
            chunks += 1;
        }
    }
    eprintln!("round-tripped {chunks} chunks {by_type:?}");
    assert!(chunks > 0);
}
