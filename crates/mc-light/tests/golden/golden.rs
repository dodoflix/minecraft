//! Differential test against the real vanilla `LevelLightEngine` (generator + scenario script: `fixtures/LightGolden.java`,
//! expectations: `fixtures/light_golden.txt`). Replays every scenario step (initial lighting, block changes, light runs,
//! chunk unload/reload with saved light) and compares layers, sky light sources, light packet bytes, point queries,
//! the `runLightUpdates` counts and the sections reported through `onLightUpdate`.

use mc_core::pos::{BlockPos, ChunkPos, SectionPos};
use mc_data::block::BlockState;
use mc_light::{ChunkMapGetter, LightLayer, ThreadedLevelLightEngine};
use mc_world::chunk::{LevelHeight, ProtoChunk};
use mc_world::chunk_packet::BitSet;
use mc_world::data_layer::DataLayer;
use mc_world::palette::{BiomeRegistry, PalettedContainerFactory};
use mc_world::status::ChunkStatus;
use mc_world::upgrade_data::UpgradeData;
use std::collections::{BTreeMap, VecDeque};
use std::path::Path;

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}

struct Scenario {
    name: String,
    height: LevelHeight,
    has_sky: bool,
    grid: (i32, i32),
    getter: ChunkMapGetter<ProtoChunk>,
    engine: ThreadedLevelLightEngine,
    removed: Option<ProtoChunk>,
    saved: BTreeMap<i32, (Option<DataLayer>, Option<DataLayer>)>,
    /// Engine runs not yet matched against a `RUNCOUNT`/`UPD` expectation pair: (count, sections hash, sections).
    runs: VecDeque<(i32, u64, usize)>,
    current_run: (i32, u64, usize),
}

fn layer_hash(layer: Option<&DataLayer>) -> String {
    layer.map_or("-".to_string(), |l| format!("{:x}", fnv(&l.data()[..])))
}

impl Scenario {
    fn new(f: &[&str], factory: &PalettedContainerFactory) -> Scenario {
        let (min_y, height): (i32, i32) = (f[2].parse().unwrap(), f[3].parse().unwrap());
        let has_sky: bool = f[4].parse().unwrap();
        let (g0, n): (i32, i32) = (f[5].parse().unwrap(), f[6].parse().unwrap());
        let height = LevelHeight::new(min_y, height);
        let mut getter = ChunkMapGetter::new();
        for cz in g0..g0 + n {
            for cx in g0..g0 + n {
                let pos = ChunkPos::new(cx, cz);
                getter.insert(
                    pos,
                    ProtoChunk::new(pos, UpgradeData::empty(), height, factory, None),
                );
            }
        }
        Scenario {
            name: f[1].to_string(),
            height,
            has_sky,
            grid: (g0, n),
            getter,
            engine: ThreadedLevelLightEngine::new(height, has_sky, 46),
            removed: None,
            saved: BTreeMap::new(),
            runs: VecDeque::new(),
            current_run: (0, 0, 0),
        }
    }

    fn positions(&self) -> Vec<ChunkPos> {
        let (g0, n) = self.grid;
        (g0..g0 + n)
            .flat_map(|cz| (g0..g0 + n).map(move |cx| ChunkPos::new(cx, cz)))
            .collect()
    }

    fn apply_section(&mut self, f: &[&str]) {
        let (cx, sy, cz): (i32, i32, i32) = (
            f[1].parse().unwrap(),
            f[2].parse().unwrap(),
            f[3].parse().unwrap(),
        );
        let chunk = self
            .getter
            .chunks
            .get_mut(&ChunkPos::pack_xz(cx, cz))
            .unwrap();
        let index = self.height.section_index_from_section_y(sy) as usize;
        let section = &mut chunk.access.sections[index];
        let mut i = 0;
        for run in f[4].split(',') {
            let (count, id) = run.split_once(':').unwrap();
            let state = BlockState::from_id(id.parse().unwrap()).unwrap();
            for _ in 0..count.parse::<usize>().unwrap() {
                let (x, y, z) = (i & 15, i >> 8, (i >> 4) & 15);
                section.set_block_state(x, y, z, state);
                i += 1;
            }
        }
        assert_eq!(i, 4096);
    }

    fn record_run(&mut self, count: i32) {
        let mut updates: Vec<i64> = self
            .engine
            .engine_mut()
            .take_light_updates()
            .into_iter()
            .map(|(layer, s)| s.as_long() * 2 + i64::from(layer == LightLayer::Sky))
            .collect();
        updates.sort_unstable();
        updates.dedup();
        let bytes: Vec<u8> = updates.iter().flat_map(|l| l.to_be_bytes()).collect();
        self.runs.push_back((count, fnv(&bytes), updates.len()));
    }

    fn run_engine(&mut self) {
        let count = self.engine.engine_mut().run_light_updates(&self.getter);
        assert!(!self.engine.engine().has_light_work());
        self.record_run(count);
    }

    fn init(&mut self) {
        let positions = self.positions();
        for pos in &positions {
            let chunk = self.getter.chunks.get_mut(&pos.pack()).unwrap();
            chunk.access.initialize_light_sources();
            chunk.set_light_engine_attached(true);
            chunk.set_persisted_status(ChunkStatus::Light);
        }
        for pos in &positions {
            self.engine
                .initialize_light(&self.getter.chunks[&pos.pack()].access, false);
        }
        let r = self.engine.run_update(&self.getter, &|_| 45);
        self.record_run(r.light_updates_run);
        for pos in &positions {
            self.engine.light_chunk(*pos, false);
        }
        let r = self.engine.run_update(&self.getter, &|_| 45);
        self.record_run(r.light_updates_run);
    }

    fn op(&mut self, f: &[&str]) {
        let (x, y, z): (i32, i32, i32) = (
            f[1].parse().unwrap(),
            f[2].parse().unwrap(),
            f[3].parse().unwrap(),
        );
        let state = BlockState::from_id(f[4].parse().unwrap()).unwrap();
        let chunk = self
            .getter
            .chunks
            .get_mut(&ChunkPos::pack_xz(x >> 4, z >> 4))
            .unwrap();
        chunk.set_block_state(
            BlockPos::new(x, y, z),
            state,
            Some(self.engine.engine_mut()),
        );
    }

    fn run_queued(&mut self) {
        let r = self.engine.run_update(&self.getter, &|_| 45);
        self.record_run(r.light_updates_run);
    }

    fn unload(&mut self, cx: i32, cz: i32) {
        let pos = ChunkPos::new(cx, cz);
        let engine = self.engine.engine();
        self.saved.clear();
        for sy in engine.get_min_light_section()..engine.get_max_light_section() {
            let section = SectionPos::of_chunk(&pos, sy);
            let keep = |layer| {
                engine
                    .get_data_layer_data(layer, section)
                    .filter(|l| !l.is_empty())
                    .map(|l| (*l).clone())
            };
            self.saved
                .insert(sy, (keep(LightLayer::Block), keep(LightLayer::Sky)));
        }
        self.removed = self.getter.chunks.remove(&pos.pack());
        self.engine.update_chunk_status(pos);
        self.run_queued();
    }

    fn reload(&mut self, cx: i32, cz: i32) {
        let pos = ChunkPos::new(cx, cz);
        let mut chunk = self.removed.take().expect("unloaded chunk");
        chunk.access.initialize_light_sources();
        self.getter.chunks.insert(pos.pack(), chunk);
        let saved: Vec<_> = std::mem::take(&mut self.saved)
            .into_iter()
            .map(|(y, (b, s))| (y, b, if self.has_sky { s } else { None }))
            .collect();
        self.engine.load_chunk_light(pos, saved);
        self.engine
            .initialize_light(&self.getter.chunks[&pos.pack()].access, true);
        self.run_queued();
        self.run_queued();
    }
}

#[test]
fn matches_vanilla_light_engine() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/light_golden.txt");
    let text = std::fs::read_to_string(&path).unwrap();
    let biomes = BiomeRegistry::new(vec!["minecraft:plains".to_string()]);
    let factory = PalettedContainerFactory::create(biomes).unwrap();
    let mut scen: Option<Scenario> = None;
    let mut ended = false;
    let (mut snaps, mut checks) = (0, 0);
    let mut label = String::new();
    let mut fail = Vec::new();
    for (lineno, line) in text.lines().enumerate() {
        let f: Vec<&str> = line.split(' ').collect();
        let ctx = || format!("line {} `{}`", lineno + 1, line);
        match f[0] {
            "SCEN" => scen = Some(Scenario::new(&f, &factory)),
            "SEC" => scen.as_mut().unwrap().apply_section(&f),
            "INIT" => {
                let s = scen.as_mut().unwrap();
                s.init();
            }
            "OP" => scen.as_mut().unwrap().op(&f),
            "RUN" => scen.as_mut().unwrap().run_engine(),
            "UNLOAD" => scen
                .as_mut()
                .unwrap()
                .unload(f[1].parse().unwrap(), f[2].parse().unwrap()),
            "RELOAD" => scen
                .as_mut()
                .unwrap()
                .reload(f[1].parse().unwrap(), f[2].parse().unwrap()),
            "RUNCOUNT" => {
                let s = scen.as_mut().unwrap();
                checks += 1;
                s.current_run = s.runs.pop_front().expect("engine run for RUNCOUNT");
                let want: i32 = f[1].parse().unwrap();
                if s.current_run.0 != want {
                    fail.push(format!(
                        "[{}] {}: runLightUpdates count {} != {want}",
                        s.name,
                        ctx(),
                        s.current_run.0
                    ));
                }
            }
            "UPD" => {
                let s = scen.as_ref().unwrap();
                checks += 1;
                let (n, h): (usize, u64) = (
                    f[1].parse().unwrap(),
                    u64::from_str_radix(f[2], 16).unwrap(),
                );
                if (s.current_run.2, s.current_run.1) != (n, h) {
                    fail.push(format!(
                        "[{}] {}: onLightUpdate sections {} / {:x} != {n} / {h:x}",
                        s.name,
                        ctx(),
                        s.current_run.2,
                        s.current_run.1
                    ));
                }
            }
            "SNAP" => {
                snaps += 1;
                label = f[1].to_string();
            }
            "L" => {
                let s = scen.as_ref().unwrap();
                checks += 1;
                let layer = if f[1] == "B" {
                    LightLayer::Block
                } else {
                    LightLayer::Sky
                };
                let section = SectionPos::of(
                    f[2].parse().unwrap(),
                    f[3].parse().unwrap(),
                    f[4].parse().unwrap(),
                );
                let got = layer_hash(
                    s.engine
                        .engine()
                        .get_data_layer_data(layer, section)
                        .as_deref(),
                );
                if got != f[5] {
                    fail.push(format!(
                        "[{}] {label}: layer {} {section}: {got} != {}",
                        s.name, f[1], f[5]
                    ));
                }
            }
            "SRC" => {
                let s = scen.as_ref().unwrap();
                checks += 1;
                let pos = ChunkPos::new(f[1].parse().unwrap(), f[2].parse().unwrap());
                let sources = &s.getter.chunks[&pos.pack()].access.sky_light_sources;
                let bytes: Vec<u8> = (0..256)
                    .flat_map(|i| sources.get_lowest_source_y(i & 15, i >> 4).to_be_bytes())
                    .collect();
                let got = format!("{:x}", fnv(&bytes));
                if got != f[3] {
                    fail.push(format!(
                        "[{}] {label}: sky sources of {pos}: {got} != {}",
                        s.name, f[3]
                    ));
                }
            }
            "PKT" => {
                let s = scen.as_ref().unwrap();
                checks += 1;
                let pos = ChunkPos::new(f[1].parse().unwrap(), f[2].parse().unwrap());
                let engine = s.engine.engine();
                let mut out = Vec::new();
                engine
                    .light_update_packet_data(pos, None, None)
                    .write(&mut out);
                let (mut f1, mut f2) = (BitSet::new(), BitSet::new());
                (0..engine.get_light_section_count() as usize)
                    .step_by(3)
                    .for_each(|i| f1.set(i));
                (1..engine.get_light_section_count() as usize)
                    .step_by(2)
                    .for_each(|i| f2.set(i));
                let mut out2 = Vec::new();
                engine
                    .light_update_packet_data(pos, Some(&f1), Some(&f2))
                    .write(&mut out2);
                let got = format!("{:x} {:x}", fnv(&out), fnv(&out2));
                if got != format!("{} {}", f[3], f[4]) {
                    fail.push(format!(
                        "[{}] {label}: light packet of {pos}: {got} != {} {}",
                        s.name, f[3], f[4]
                    ));
                }
            }
            "Q" => {
                let s = scen.as_ref().unwrap();
                checks += 1;
                let v: Vec<i32> = f[2..].iter().map(|n| n.parse().unwrap()).collect();
                let pos = BlockPos::new(v[0], v[1], v[2]);
                let reader = s.engine.engine().reader();
                let (got, want) = match f[1] {
                    "B" => (reader.get_light_value(LightLayer::Block, pos), v[3]),
                    "S" => (reader.get_light_value(LightLayer::Sky, pos), v[3]),
                    _ => (reader.get_raw_brightness(pos, v[3]), v[4]),
                };
                if got != want {
                    fail.push(format!(
                        "[{}] {label}: query {} {pos}: {got} != {want}",
                        s.name, f[1]
                    ));
                }
            }
            "END" => ended = true,
            _ => panic!("bad line {}", ctx()),
        }
    }
    assert!(ended);
    eprintln!(
        "{snaps} snapshots, {checks} checks, {} failures",
        fail.len()
    );
    for m in fail.iter().take(40) {
        eprintln!("{m}");
    }
    assert!(fail.is_empty(), "{} mismatches", fail.len());
}
