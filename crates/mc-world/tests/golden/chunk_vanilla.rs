//! Opt-in (`cargo test -p mc-world --test chunk_vanilla -- --ignored`): rewrites every chunk of the vanilla world
//! `run/servers/worldref` with our code, starts the REAL vanilla server on the rewritten world, requires a clean log,
//! lets vanilla re-save the chunks it loaded and checks that vanilla's NBT equals ours (so vanilla read the very same
//! blocks, biomes, heightmaps, ticks and structures we wrote).


use crate::chunk_common::*;
use mc_nbt::Tag;
use mc_world::data_layer::DataLayer;
use mc_world::serial::{LoadedChunk, ReadContext, SerializableChunkData};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

/// Vanilla relights chunks on load, so light arrays (and light-only sections) are compared nowhere.
fn strip_light(t: &mut mc_nbt::CompoundTag) {
    t.remove("isLightOn");
    // scheduled tick delays depend on how many ticks the (non-deterministic) run lasted when it saved
    for key in ["block_ticks", "fluid_ticks"] {
        if let Some(Tag::List(ticks)) = t.get_mut(key) {
            for tick in ticks.iter_mut() {
                tick.as_compound_mut().unwrap().remove("t");
            }
        }
    }
    if let Some(Tag::List(sections)) = t.get_mut("sections") {
        let kept: Vec<Tag> = sections
            .iter()
            .cloned()
            .filter_map(|s| {
                let mut c = s.as_compound().unwrap().clone();
                c.remove("BlockLight");
                c.remove("SkyLight");
                c.contains("block_states").then_some(Tag::Compound(c))
            })
            .collect();
        *sections = kept.into();
    }
}

/// Freezes everything that would make two vanilla runs differ (random ticks, time, weather).
fn freeze_rules(world: &Path) {
    let path = world.join("data/minecraft/game_rules.dat");
    let bytes = std::fs::read(&path).unwrap();
    let mut tag = mc_nbt::compress::read_compressed(&bytes, &mut mc_nbt::NbtAccounter::unlimited_heap()).unwrap();
    let data = tag.get_compound_mut("data").unwrap();
    data.put_int("minecraft:random_tick_speed", 0);
    data.put_boolean("minecraft:advance_time", false);
    data.put_boolean("minecraft:advance_weather", false);
    data.put_boolean("minecraft:spawn_mobs", false);
    std::fs::write(&path, mc_nbt::compress::write_compressed(&tag)).unwrap();
}

/// Starts vanilla on `server/world`, lets it load + save everything, stops it and returns its log.
fn run_vanilla(dir: &Path) -> String {
    let server = mc_testkit::Server::vanilla()
        .dir(dir)
        .properties([
            "online-mode=false",
            "enforce-secure-profile=false",
            "white-list=false",
            "spawn-protection=0",
            "view-distance=6",
            "simulation-distance=6",
            "gamemode=creative",
            "difficulty=normal",
        ])
        .start()
        .expect("start vanilla");
    server.cmd("save-all flush");
    assert!(server.wait_log("Saved the game", 60).is_some(), "vanilla did not finish saving: {}", server.log());
    server.stop();
    std::fs::read_to_string(server.log_path()).unwrap()
}

/// A server directory of its own (removed with the returned guard) holding a copy of `src` as `world`.
fn prepare(src: &Path) -> (tempfile::TempDir, PathBuf) {
    let server = tempfile::tempdir().unwrap();
    let world = server.path().join("world");
    copy_dir(src, &world);
    std::fs::remove_dir_all(world.join("logs")).ok();
    freeze_rules(&world);
    (server, world)
}

#[test]
#[ignore = "starts the real vanilla server twice (~1.5 min)"]
fn vanilla_accepts_rewritten_world() {
    let src = world_dir();
    // needs the local, hand-built world (no script creates it); java and the jar are required by the testkit
    if !src.exists() {
        eprintln!("SKIP: {} absent", src.display());
        return;
    }
    let (dir_a, world_a) = prepare(&src);
    let (dir_b, world_b) = prepare(&src);

    let factory = factory();
    let all_known = |_: &str| true;
    for (dim, height, has_sky) in DIMENSIONS {
        let folder = src.join(format!("dimensions/minecraft/{dim}/region"));
        let out_folder = world_b.join(format!("dimensions/minecraft/{dim}/region"));
        std::fs::remove_dir_all(&out_folder).unwrap();
        let mut out_storage = storage(&out_folder, "chunk");
        let orig_folder = world_a.join(format!("dimensions/minecraft/{dim}/region"));
        std::fs::remove_dir_all(&orig_folder).unwrap();
        let mut orig_storage = storage(&orig_folder, "chunk");
        for (pos, tag) in read_all(&folder, "chunk") {
            // scheduled ticks make two vanilla runs diverge (fluids flow while the run lasts); they are covered by the
            // `ticks_*` tests, so run both worlds without them
            let mut stripped = tag.clone();
            stripped.put("block_ticks", mc_nbt::ListTag::new());
            stripped.put("fluid_ticks", mc_nbt::ListTag::new());
            orig_storage.write(&pos, Some(&stripped)).unwrap();
            let data = SerializableChunkData::parse(height, &factory, &stripped).unwrap().unwrap();
            let game_time = data.last_update_time;
            let ctx = ReadContext { level_height: height, has_sky_light: has_sky, factory: &factory, is_known_structure: &all_known };
            let loaded = data.read(&ctx, pos);
            let light: HashMap<i32, (Option<DataLayer>, Option<DataLayer>)> =
                loaded.light.iter().map(|(y, b, s)| (*y, (b.clone(), s.clone()))).collect();
            let light_fn = |y: i32| light.get(&y).cloned().unwrap_or((None, None));
            let rewritten = match &loaded.chunk {
                LoadedChunk::Proto(p) => {
                    let be: Vec<_> = p.access.block_entities.values().cloned().collect();
                    SerializableChunkData::from_proto_chunk(p, game_time, &light_fn, be)
                }
                LoadedChunk::Level(l) => {
                    SerializableChunkData::from_level_chunk(l, game_time, &light_fn, loaded.post_load_block_entities.clone())
                }
            };
            let mut out = rewritten.write(&factory).unwrap();
            // keep the file comparable to untouched vanilla files: drop heightmaps priming added on load
            let hm = out.get_compound_mut("Heightmaps").unwrap();
            for ty in mc_world::heightmap::HeightmapType::ALL {
                if tag.get_compound("Heightmaps").is_none_or(|h| !h.contains(ty.serialization_key())) {
                    hm.remove(ty.serialization_key());
                }
            }
            out_storage.write(&pos, Some(&out)).unwrap();
        }
        out_storage.close().unwrap();
        orig_storage.close().unwrap();
    }

    let _log_a = run_vanilla(dir_a.path());
    let log_b = run_vanilla(dir_b.path());
    for line in log_b.lines() {
        assert!(
            !(line.contains("ERROR") || line.contains("Failed to read chunk") || line.contains("Recoverable errors") || line.contains("Exception")),
            "vanilla complained about the rewritten world: {line}"
        );
    }

    let mut compared = 0;
    for (dim, _, _) in DIMENSIONS {
        let dir = format!("dimensions/minecraft/{dim}/region");
        let chunks_a = read_all(&world_a.join(&dir), "chunk");
        let chunks_b: HashMap<_, _> =
            read_all(&world_b.join(&dir), "chunk").into_iter().map(|(p, t)| ((p.x, p.z), t)).collect();
        assert_eq!(chunks_a.len(), chunks_b.len(), "{dim}: chunk count");
        for (pos, tag_a) in chunks_a {
            let mut a = normalize(&tag_a);
            let mut b = normalize(chunks_b.get(&(pos.x, pos.z)).unwrap_or_else(|| panic!("{dim} {pos} missing")));
            // vanilla stamps the save time and counts inhabited time while the chunk is loaded
            for t in [&mut a, &mut b] {
                t.remove("LastUpdate");
                t.remove("InhabitedTime");
                strip_light(t);
            }
            if let Some(d) = diff(&Tag::Compound(a), &Tag::Compound(b), "") {
                panic!("{dim} chunk {pos}: vanilla's save after loading our file differs from its save after loading the original: {d}");
            }
            compared += 1;
        }
    }
    eprintln!("vanilla re-saved {compared} chunks identically for original and rewritten world");
    assert!(compared > 0);
}
