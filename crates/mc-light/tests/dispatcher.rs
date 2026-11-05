//! Differential test of the light task order against the real vanilla `ThreadedLevelLightEngine` behind its real
//! `ChunkTaskDispatcher` (generator + schedule description: `fixtures/DispatcherGolden.java`, expectations:
//! `fixtures/dispatcher_golden.txt`). Replays randomized interleavings of light tasks of several chunks with differing
//! queue levels (initialize/light/barrier/checkBlock/…) and compares, after every light-thread visit, the order in which
//! the vanilla futures complete, the sections reported through `onLightUpdate`, every chunk's light layers and its
//! `ClientboundLightUpdatePacketData` bytes.

use mc_core::pos::{BlockPos, ChunkPos, SectionPos};
use mc_data::block::BlockState;
use mc_light::{ChunkMapGetter, LightCompletion, LightLayer, ThreadedLevelLightEngine};
use mc_world::chunk::{LevelHeight, ProtoChunk};
use mc_world::data_layer::DataLayer;
use mc_world::palette::{BiomeRegistry, PalettedContainerFactory};
use mc_world::status::ChunkStatus;
use mc_world::upgrade_data::UpgradeData;
use std::collections::HashMap;
use std::path::Path;

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |h, &b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3))
}

struct Scenario {
    name: String,
    height: LevelHeight,
    n: i32,
    getter: ChunkMapGetter<ProtoChunk>,
    engine: ThreadedLevelLightEngine,
    levels: HashMap<i64, i32>,
    barriers: u64,
    /// Completion tokens and `onLightUpdate` sections of the last PUMP.
    done: Vec<String>,
    updates: Vec<i64>,
}

impl Scenario {
    fn new(f: &[&str], factory: &PalettedContainerFactory) -> Scenario {
        let (min_y, height): (i32, i32) = (f[3].parse().unwrap(), f[4].parse().unwrap());
        let has_sky: bool = f[2].parse().unwrap();
        let n: i32 = f[5].parse().unwrap();
        let height = LevelHeight::new(min_y, height);
        let mut getter = ChunkMapGetter::new();
        for cz in 0..n {
            for cx in 0..n {
                let pos = ChunkPos::new(cx, cz);
                getter.insert(pos, ProtoChunk::new(pos, UpgradeData::empty(), height, factory, None));
            }
        }
        Scenario {
            name: f[1].to_string(),
            height,
            n,
            getter,
            engine: ThreadedLevelLightEngine::new(height, has_sky, 46),
            levels: HashMap::new(),
            barriers: 0,
            done: Vec::new(),
            updates: Vec::new(),
        }
    }

    fn apply_section(&mut self, f: &[&str]) {
        let (cx, sy, cz): (i32, i32, i32) = (f[1].parse().unwrap(), f[2].parse().unwrap(), f[3].parse().unwrap());
        let chunk = self.getter.chunks.get_mut(&ChunkPos::pack_xz(cx, cz)).unwrap();
        let section = &mut chunk.access.sections[self.height.section_index_from_section_y(sy) as usize];
        let mut i = 0;
        for run in f[4].split(',') {
            let (count, id) = run.split_once(':').unwrap();
            let state = BlockState::from_id(id.parse().unwrap()).unwrap();
            for _ in 0..count.parse::<usize>().unwrap() {
                section.set_block_state((i & 15) as i32, (i >> 8) as i32, ((i >> 4) & 15) as i32, state);
                i += 1;
            }
        }
        assert_eq!(i, 4096);
    }

    fn init(&mut self) {
        for chunk in self.getter.chunks.values_mut() {
            chunk.access.initialize_light_sources();
            chunk.set_light_engine_attached(true);
            chunk.set_persisted_status(ChunkStatus::Light);
        }
    }

    fn pump(&mut self) {
        self.done.clear();
        if self.engine.has_pending_work() {
            let levels = &self.levels;
            let queue_level = |key: i64| levels.get(&key).copied().unwrap_or(45).min(45);
            let result = self.engine.run_update(&self.getter, &queue_level);
            for completion in result.completions {
                self.done.push(match completion {
                    LightCompletion::Initialized(p) => format!("I:{}:{}", p.x, p.z),
                    LightCompletion::Lit(p) => format!("L:{}:{}", p.x, p.z),
                    LightCompletion::Barrier(id) => id.to_string(),
                });
            }
        }
        self.updates = self
            .engine
            .engine_mut()
            .take_light_updates()
            .into_iter()
            .map(|(layer, s)| s.as_long() * 2 + i64::from(layer == LightLayer::Sky))
            .collect();
        self.updates.sort_unstable();
        self.updates.dedup();
    }

    fn chunk_hashes(&self, pos: ChunkPos) -> (u64, u64) {
        let engine = self.engine.engine();
        let mut all = Vec::new();
        for layer in [LightLayer::Block, LightLayer::Sky] {
            for sy in engine.get_min_light_section()..engine.get_max_light_section() {
                match engine.get_data_layer_data(layer, SectionPos::of_chunk(&pos, sy)) {
                    None => all.push(0),
                    Some(d) => {
                        all.push(1);
                        let d: &DataLayer = &d;
                        all.extend_from_slice(&d.data()[..]);
                    }
                }
            }
        }
        let mut packet = Vec::new();
        engine.light_update_packet_data(pos, None, None).write(&mut packet);
        (fnv(&all), fnv(&packet))
    }
}

#[test]
fn light_task_order_matches_vanilla_dispatcher() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dispatcher_golden.txt");
    let text = std::fs::read_to_string(&path).unwrap();
    let biomes = BiomeRegistry::new(vec!["minecraft:plains".to_string()]);
    let factory = PalettedContainerFactory::create(biomes).unwrap();
    let mut scen: Option<Scenario> = None;
    let mut ended = false;
    let (mut pumps, mut checks) = (0, 0);
    let mut fail = Vec::new();
    let mut barrier_pos: HashMap<u64, ChunkPos> = HashMap::new();
    let mut expected_done: Vec<String> = Vec::new();
    for (lineno, line) in text.lines().enumerate() {
        let f: Vec<&str> = line.split(' ').collect();
        let ctx = || format!("line {} `{}`", lineno + 1, line);
        let int = |i: usize| -> i32 { f[i].parse().unwrap() };
        let pos = || ChunkPos::new(f[1].parse().unwrap(), f[2].parse().unwrap());
        if f[0] == "SCEN" {
            scen = Some(Scenario::new(&f, &factory));
            barrier_pos.clear();
            continue;
        }
        if f[0] == "END" {
            ended = true;
            continue;
        }
        let Some(s) = scen.as_mut() else { panic!("no scenario at {}", ctx()) };
        match f[0] {
            "SEC" => s.apply_section(&f),
            "INIT" => s.init(),
            "ENDSCEN" => {}
            "QL" => {
                s.levels.insert(pos().pack(), int(3));
            }
            "INITLIGHT" => {
                let lighted: bool = f[3].parse().unwrap();
                s.engine.initialize_light(&s.getter.chunks[&pos().pack()].access, lighted);
            }
            "LIGHTCHUNK" => s.engine.light_chunk(pos(), f[3].parse().unwrap()),
            "SET" => {
                let (x, y, z) = (int(1), int(2), int(3));
                let state = BlockState::from_id(int(4) as u16 as _).unwrap();
                let chunk = s.getter.chunks.get_mut(&ChunkPos::pack_xz(x >> 4, z >> 4)).unwrap();
                chunk.set_block_state(BlockPos::new(x, y, z), state, Some(&mut s.engine));
            }
            "BARRIER" => {
                let id = s.engine.wait_for_pending_tasks(pos());
                assert_eq!(id, s.barriers);
                s.barriers += 1;
                barrier_pos.insert(id, pos());
            }
            "PROP" => s.engine.propagate_light_sources(pos()),
            "ENABLE" => s.engine.set_light_enabled(pos(), f[3].parse().unwrap()),
            "RETAIN" => s.engine.retain_data(pos(), f[3].parse().unwrap()),
            "SECT" => s.engine.update_section_status(SectionPos::of(int(1), int(2), int(3)), f[4].parse().unwrap()),
            "STATUS" => s.engine.update_chunk_status(pos()),
            "PUMP" => {
                pumps += 1;
                s.pump();
                // Barrier completions carry the chunk the Java side logged (`B:<n>:<cx>:<cz>`).
                for token in &mut s.done {
                    if let Ok(id) = token.parse::<u64>() {
                        let p = barrier_pos[&id];
                        *token = format!("B:{id}:{}:{}", p.x, p.z);
                    }
                }
            }
            "DONE" => {
                checks += 1;
                expected_done = f[1..].iter().filter(|t| !t.is_empty()).map(|t| t.to_string()).collect();
                if s.done != expected_done {
                    fail.push(format!("[{}] {}: completion order {:?} != {:?}", s.name, ctx(), s.done, expected_done));
                }
            }
            "UPD" => {
                checks += 1;
                let bytes: Vec<u8> = s.updates.iter().flat_map(|l| l.to_be_bytes()).collect();
                let got = (s.updates.len(), fnv(&bytes));
                let want = (int(1) as usize, u64::from_str_radix(f[2], 16).unwrap());
                if got != want {
                    fail.push(format!("[{}] {}: onLightUpdate sections {got:x?} != {want:x?}", s.name, ctx()));
                }
            }
            "CH" => {
                checks += 1;
                let (layers, packet) = s.chunk_hashes(pos());
                let got = format!("{layers:x} {packet:x}");
                if got != format!("{} {}", f[3], f[4]) {
                    fail.push(format!("[{}] {}: light layers/packet {got} != {} {}", s.name, ctx(), f[3], f[4]));
                }
            }
            _ => panic!("bad line {}", ctx()),
        }
        let _ = s.n;
    }
    let _ = expected_done;
    assert!(ended);
    eprintln!("{pumps} pumps, {checks} checks, {} failures", fail.len());
    for m in fail.iter().take(40) {
        eprintln!("{m}");
    }
    assert!(fail.is_empty(), "{} mismatches", fail.len());
}
