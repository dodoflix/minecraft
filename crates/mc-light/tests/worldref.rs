//! Differential test: relight the vanilla-generated world `run/servers/worldref` from its block data and compare with the
//! light arrays vanilla saved (every section of every fully lit chunk whose 8 neighbors are full chunks too).
//! Skips when the world is absent.

use mc_core::ident::Identifier;
use mc_core::pos::{ChunkPos, SectionPos};
use mc_light::{ChunkMapGetter, LightChunk, LightLayer, ThreadedLevelLightEngine};
use mc_world::chunk::{LevelChunk, LevelHeight, ProtoChunk};
use mc_world::data_layer::DataLayer;
use mc_world::palette::{BiomeRegistry, PalettedContainerFactory};
use mc_world::region::{RegionFileStorage, RegionStorageInfo};
use mc_world::serial::{LoadedChunk, ReadContext, SerializableChunkData};
use mc_world::status::ChunkStatus;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const DIMENSIONS: [(&str, LevelHeight, bool); 3] = [
    ("overworld", LevelHeight::new(-64, 384), true),
    ("the_nether", LevelHeight::new(0, 256), false),
    ("the_end", LevelHeight::new(0, 256), true),
];

fn world_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../run/servers/worldref/world")
}

fn factory() -> PalettedContainerFactory {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/generated/data/minecraft/worldgen/biome");
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| format!("minecraft:{}", e.unwrap().file_name().to_str().unwrap().trim_end_matches(".json")))
        .collect();
    names.sort();
    PalettedContainerFactory::create(BiomeRegistry::new(names)).unwrap()
}

fn read_all(folder: &Path) -> Vec<(ChunkPos, mc_nbt::CompoundTag)> {
    let mut regions: Vec<(i32, i32)> = Vec::new();
    for e in std::fs::read_dir(folder).into_iter().flatten() {
        let name = e.unwrap().file_name().into_string().unwrap();
        if let Some(rest) = name.strip_prefix("r.").and_then(|r| r.strip_suffix(".mca")) {
            let (x, z) = rest.split_once('.').unwrap();
            regions.push((x.parse().unwrap(), z.parse().unwrap()));
        }
    }
    regions.sort();
    let info = RegionStorageInfo::new("test", Identifier::parse("minecraft:overworld").unwrap(), "chunk");
    let mut storage = RegionFileStorage::new(info, folder, false);
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

enum Chunk {
    Proto(ProtoChunk),
    Level(LevelChunk),
}

impl Chunk {
    fn access_mut(&mut self) -> &mut mc_world::chunk::ChunkAccess {
        match self {
            Chunk::Proto(p) => &mut p.access,
            Chunk::Level(l) => &mut l.access,
        }
    }
}

impl LightChunk for Chunk {
    fn get_block_state(&self, pos: mc_core::pos::BlockPos) -> mc_data::block::BlockState {
        match self {
            Chunk::Proto(p) => LightChunk::get_block_state(p, pos),
            Chunk::Level(l) => LightChunk::get_block_state(l, pos),
        }
    }
    fn find_block_light_sources(&self, consumer: &mut dyn FnMut(mc_core::pos::BlockPos, mc_data::block::BlockState)) {
        match self {
            Chunk::Proto(p) => p.find_block_light_sources(consumer),
            Chunk::Level(l) => l.find_block_light_sources(consumer),
        }
    }
    fn get_sky_light_sources(&self) -> &mc_world::sky_light_sources::ChunkSkyLightSources {
        match self {
            Chunk::Proto(p) => p.get_sky_light_sources(),
            Chunk::Level(l) => l.get_sky_light_sources(),
        }
    }
}

fn nibbles(layer: Option<&DataLayer>) -> Vec<u8> {
    (0..4096usize).map(|i| layer.map_or(0, |l| l.get((i & 15) as i32, (i >> 8) as i32, ((i >> 4) & 15) as i32) as u8)).collect()
}

#[test]
fn relight_matches_saved_light() {
    let world = world_dir();
    if !world.exists() {
        eprintln!("SKIP: {} absent", world.display());
        return;
    }
    let factory = factory();
    let all_known = |_: &str| true;
    let mut grand = (0usize, 0usize, 0usize);
    for (dim, height, has_sky) in DIMENSIONS {
        let folder = world.join(format!("dimensions/minecraft/{dim}/region"));
        let mut getter: ChunkMapGetter<Chunk> = ChunkMapGetter::new();
        let mut saved: HashMap<ChunkPos, (bool, ChunkStatus, HashMap<i32, (Option<DataLayer>, Option<DataLayer>)>)> = HashMap::new();
        for (pos, tag) in read_all(&folder) {
            let data = SerializableChunkData::parse(height, &factory, &tag).unwrap().expect("status");
            let status = data.chunk_status;
            let lit = data.light_correct;
            let ctx = ReadContext { level_height: height, has_sky_light: has_sky, factory: &factory, is_known_structure: &all_known };
            let loaded = data.read(&ctx, pos);
            let light = loaded.light.into_iter().map(|(y, b, s)| (y, (b, s))).collect();
            saved.insert(pos, (lit, status, light));
            getter.insert(
                pos,
                match loaded.chunk {
                    LoadedChunk::Proto(p) => Chunk::Proto(p),
                    LoadedChunk::Level(l) => Chunk::Level(l),
                },
            );
        }
        if saved.is_empty() {
            continue;
        }
        let positions: Vec<ChunkPos> = {
            let mut v: Vec<_> = saved.keys().copied().collect();
            v.sort_by_key(|p| (p.z, p.x));
            v
        };
        for pos in &positions {
            getter.chunks.get_mut(&pos.pack()).unwrap().access_mut().initialize_light_sources();
        }
        let mut engine = ThreadedLevelLightEngine::new(height, has_sky);
        for pos in &positions {
            let access = match getter.chunks.get_mut(&pos.pack()).unwrap() {
                Chunk::Proto(p) => &p.access,
                Chunk::Level(l) => &l.access,
            };
            engine.initialize_light(access, false);
        }
        while engine.pending_task_count() > 0 {
            engine.run_update(&getter);
        }
        for pos in &positions {
            engine.light_chunk(*pos, false);
        }
        while engine.pending_task_count() > 0 {
            engine.run_update(&getter);
        }
        assert!(!engine.has_pending_work());

        let is_full = |p: ChunkPos| saved.get(&p).is_some_and(|(_, s, _)| *s == ChunkStatus::Full);
        let (mut compared_chunks, mut bad_chunks, mut sections, mut stale_in_opaque) = (0, 0, 0, 0);
        for pos in &positions {
            let (lit, status, light) = &saved[pos];
            let surrounded = (-1..=1).all(|dz| (-1..=1).all(|dx| is_full(ChunkPos::new(pos.x + dx, pos.z + dz))));
            if !*lit || *status != ChunkStatus::Full || !surrounded {
                continue;
            }
            compared_chunks += 1;
            let mut bad = Vec::new();
            for section_y in engine.engine().get_min_light_section()..engine.engine().get_max_light_section() {
                let section = SectionPos::of_chunk(pos, section_y);
                let (sb, ss) = light.get(&section_y).cloned().unwrap_or((None, None));
                let layers = [(LightLayer::Block, sb), (LightLayer::Sky, if has_sky { ss } else { None })];
                for (layer, expected) in layers {
                    if layer == LightLayer::Sky && !has_sky {
                        continue;
                    }
                    let got = engine.engine().get_data_layer_data(layer, section);
                    sections += 1;
                    let (mut e, g) = (nibbles(expected.as_ref()), nibbles(got.as_deref()));
                    if expected.is_none() {
                        // Vanilla drops a section's layer when its last non-empty neighbor section goes away (chunk unloading
                        // order at save time), so "absent" only tells us the sky was open or the layer was empty.
                        let homogeneous = g.iter().all(|&n| n == g[0]);
                        let plausible = if layer == LightLayer::Block { g.iter().all(|&n| n == 0) } else { homogeneous && (g[0] == 0 || g[0] == 15) };
                        if plausible {
                            e = g.clone();
                        }
                    } else {
                        // Vanilla can leave stale light inside fully opaque blocks placed while the section was not stored
                        // (End pillars across a chunk border); it is not observable, so ignore those cells.
                        let chunk = &getter.chunks[&pos.pack()];
                        for (i, (en, gn)) in e.iter_mut().zip(&g).enumerate() {
                            if en != gn {
                                let bp = mc_core::pos::BlockPos::new(pos.x * 16 + (i & 15) as i32, section_y * 16 + (i >> 8) as i32, pos.z * 16 + ((i >> 4) & 15) as i32);
                                if chunk.get_block_state(bp).get_light_block() == 15 {
                                    stale_in_opaque += 1;
                                    *en = *gn;
                                }
                            }
                        }
                    }
                    if e != g {
                        let diffs = e.iter().zip(&g).filter(|(a, b)| a != b).count();
                        let hist = |v: &[u8]| {
                            let mut h = [0usize; 16];
                            v.iter().for_each(|&n| h[n as usize] += 1);
                            h.iter().enumerate().filter(|(_, c)| **c > 0).map(|(i, c)| format!("{i}:{c}")).collect::<Vec<_>>().join(",")
                        };
                        let firsts: Vec<String> = e
                            .iter()
                            .zip(&g)
                            .enumerate()
                            .filter(|(_, (a, b))| a != b)
                            .take(4)
                            .map(|(i, (a, b))| format!("(x{} y{} z{}: want {a} got {b})", i & 15, i >> 8, (i >> 4) & 15))
                            .collect();
                        bad.push(format!(
                            "{layer:?} y={section_y}: {diffs} nibbles differ; saved={} stored={} want[{}] got[{}] {}",
                            expected.is_some(),
                            got.is_some(),
                            hist(&e),
                            hist(&g),
                            firsts.join(" ")
                        ));
                    }
                }
            }
            if !bad.is_empty() && std::env::var("DUMP").is_ok() {
                let access = match getter.chunks.get(&pos.pack()).unwrap() {
                    Chunk::Proto(p) => &p.access,
                    Chunk::Level(l) => &l.access,
                };
                for section_y in engine.engine().get_min_light_section()..engine.engine().get_max_light_section() {
                    let idx = section_y - height.min_section_y();
                    let non_empty = (idx >= 0 && (idx as usize) < access.sections.len()).then(|| access.sections[idx as usize].non_empty_block_count());
                    let (_, ss) = light.get(&section_y).cloned().unwrap_or((None, None));
                    let got = engine.engine().get_data_layer_data(LightLayer::Sky, SectionPos::of_chunk(pos, section_y));
                    let hist = |v: &[u8]| {
                        let mut h = [0usize; 16];
                        v.iter().for_each(|&n| h[n as usize] += 1);
                        h.iter().enumerate().filter(|(_, c)| **c > 0).map(|(i, c)| format!("{i}:{c}")).collect::<Vec<_>>().join(",")
                    };
                    eprintln!("  y={section_y} nonempty={non_empty:?} saved[{}] got[{}] gotstored={}", hist(&nibbles(ss.as_ref())), hist(&nibbles(got.as_deref())), got.is_some());
                }
                if let Ok(spec) = std::env::var("COLUMN") {
                    let v: Vec<i32> = spec.split(',').map(|n| n.parse().unwrap()).collect();
                    let (bx, bz) = (v[0], v[1]);
                    for y in v[2]..=v[3] {
                        let bp = mc_core::pos::BlockPos::new(bx, y, bz);
                        let cpos = ChunkPos::new(bx >> 4, bz >> 4);
                        let st = getter.chunks[&cpos.pack()].get_block_state(bp);
                        let sec = y >> 4;
                        let sv = saved[&cpos].2.get(&sec).and_then(|(_, s)| s.as_ref().map(|l| l.get(bx & 15, y & 15, bz & 15)));
                        eprintln!("    ({bx},{y},{bz}) {st} saved={sv:?} got={}", engine.engine().get_light_value(LightLayer::Sky, bp));
                    }
                }
                let src = access.sky_light_sources.get_highest_lowest_source_y();
                eprintln!("  highest lowest source y = {src}");
            }
            if !bad.is_empty() {
                bad_chunks += 1;
                eprintln!("{dim} chunk {pos}: {}", bad.join("; "));
            }
        }
        eprintln!("{dim}: compared {compared_chunks} chunks / {sections} light sections, {bad_chunks} chunks differ ({stale_in_opaque} stale cells inside opaque blocks ignored)");
        grand.0 += compared_chunks;
        grand.1 += sections;
        grand.2 += bad_chunks;
    }
    assert!(grand.0 > 0, "no chunk qualified for the comparison");
    assert_eq!(grand.2, 0, "{} of {} chunks differ", grand.2, grand.0);
}
