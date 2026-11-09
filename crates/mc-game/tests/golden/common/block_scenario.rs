//! Rust replay of the block-oracle scenario DSL on a TICKING detached [`ServerLevel`] (twin of
//! `tests/oracle/EntityOracleDriver.java`; vanilla output goes to `tests/fixtures/entity_oracle/NAME.txt`).
//!
//! A sibling's golden test is a few lines:
//! ```ignore
//! mod common;
//! #[test]
//! fn lever_lamp() {
//!     common::block_scenario::check("blk_redstone_lever_lamp"); // replays tests/oracle/scenarios/NAME.txt, compares with the fixture
//! }
//! ```
//! Generate the fixture with `scripts/block-oracle.sh blk_redstone_lever_lamp` (runs the REAL vanilla level).
//!
//! Supported commands (same text as the Java driver, see its header): `level flat|void`, `dimension overworld|the_nether|the_end`, `seed`, `difficulty`, `preload R`, `features NAME,..`,
//! `note`, `cmd weather rain|clear TICKS`, `gamerule NAME VALUE`, `time N` (the overworld clock: sky light and sun angle follow from the next tick), `block X Y Z STATE`, `fill X1 Y1 Z1 X2 Y2 Z2 STATE`, `level_seed N`, `entity_counter N` (the next entity gets id N+1; the level's own counter starts at 0), `rand_value N`,
//! `tick_chunk CX CZ SPEED`, `random_tick X Y Z`, `be_nbt X Y Z SNBT`, `bonemeal X Y Z` (`BONEMEAL true|false`), `destroy X Y Z` (`Level.destroyBlock`, `DESTROY true|false`), `tick [N]`, `watch WHAT..`, `dump WHAT..` with WHAT in
//! `blocks:x1,y1,z1,x2,y2,z2` (non-air blocks as `B x y z state`), `be:x,y,z` (`BE x y z SNBT`), `beu:x,y,z` (`BEU x y z TYPE SNBT` = update packet tag), `bes:cx,cz` (`BES`/`BEM x y z type`/`BET x y z`/`BETP x y z`: block entity map and ticker list), `chunkbes:cx,cz` (`CHUNKBES`/`BEP x y z type`: the block entity list of the chunk packet; `BEM` is the map's iteration order, vanilla's fastutil order), `ticks[:x1,y1,z1,x2,y2,z2]` (`TICK block|fluid x y z id
//! delay=N prio=P`, execution order), `events` (`EV ...` lines of level events / sounds / block events broadcast since the last
//! dump: [`ServerLevel`]'s `tick_state.broadcast_log` here, `VanillaLevel.RecordingPlayerList` in Java). Everything else panics
//! with the line number, so a scenario never silently runs half.
//!
//! Notes: the level ticks like `ServerLevel::tick` (scheduled block/fluid ticks, block events, random ticks, block entity
//! tickers; `random_tick_speed` defaults to 0 like the oracle), game time advances one per tick. Prefer `tick_chunk` + `rand_value` to
//! `gamerule random_tick_speed`: whole-level random ticking depends on chunk iteration order, which the two servers do not share.
//! `gamerule` goes through the real `GameRules` (all rules).
#[path = "block_scenario_actors.rs"]
mod actors;
use super::entity_oracle::{Fixture, Options, compare, fixture_dir, scenario_dir};
use mc_core::ident::Identifier;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::block::{Block, BlockState};
use mc_game::server::level::level_events::BroadcastRecord;
use mc_game::server::level::level_tick::{LevelTickEnv, WeatherState};
use mc_game::server::level::world_gen::{ChunkGeneratorBackend, WorldGenRegion};
use mc_game::server::level::{ServerLevel, ServerLevelParams};
use mc_game::server::shared::ServerShared;
use mc_game::world::entity::Entity;
use mc_game::world::level::block::BlockStateExt;
use mc_game::world::level::gamerules::{AnyGameRule, GameRules};
use mc_game::world::level::material::FluidStateExt;
use mc_game::world::level::{BlockGetter, LevelAccessor};
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::codec::{Decode, read_varint};
use mc_protocol::packet::Packet;
use mc_protocol::packets::play::{
    ClientboundBlockEventPacket, ClientboundLevelEventPacket, ClientboundSoundPacket,
    SoundEventHolder, SoundSource,
};
use mc_world::chunk::{LevelHeight, ProtoChunk, SectionsView};
use mc_world::heightmap::{Heightmap, HeightmapType};
use mc_world::level_storage::{Difficulty, FeatureFlagSet};
use mc_world::palette::{BiomeRegistry, PalettedContainerFactory};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Replay `tests/oracle/scenarios/NAME.txt` and return the lines the Java driver would print.
pub fn run_scenario(name: &str) -> Vec<String> {
    let path = scenario_dir().join(format!("{name}.txt"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("scenario {}: {e}", path.display()));
    run_scenario_text(&text)
}

/// Replay and compare with `tests/fixtures/entity_oracle/NAME.txt` (panics with the first differences).
pub fn check(name: &str) {
    check_with(name, &Options::default());
}

pub fn check_with(name: &str, opts: &Options) {
    let actual = run_scenario(name);
    // `BLOCK_SCENARIO_ACTUAL=DIR` writes the replay output to DIR/NAME.txt, to diff it against the fixture by hand
    if let Some(dir) = std::env::var_os("BLOCK_SCENARIO_ACTUAL") {
        std::fs::write(
            std::path::Path::new(&dir).join(format!("{name}.txt")),
            actual.join("\n") + "\n",
        )
        .expect("write the replay output");
    }
    let fixture = Fixture::load(name);
    // `C` lines are the command feedback of the driver's `time`/`gamerule` verbs ("Set minecraft:overworld to N tick(s)"): the
    // replay changes the same state without a command, so the text is not part of the comparison
    let mut opts = opts.clone();
    opts.ignore_kinds.push("C");
    if let Err(report) = compare(&fixture.lines, &actual, &opts) {
        panic!(
            "block oracle scenario `{name}` differs from {}:\n{report}",
            fixture_dir().join(format!("{name}.txt")).display()
        );
    }
}

pub fn run_scenario_text(text: &str) -> Vec<String> {
    let mut s = Scenario::new();
    // precipitation and the bone meal feature lists read the biome: only the weather and bone meal scenarios pay for the real
    // biome registry
    s.real_biomes = text.lines().any(|l| {
        let l = l.trim_start();
        l.starts_with("cmd weather") || l.starts_with("bonemeal")
    });
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        s.line_no = i + 1;
        s.line = line.to_string();
        s.run(&tokens(line));
    }
    s.out
}

/// Java `EntityOracleDriver.tokens`: whitespace separated, `[..]`, `{..}` and quotes keep their spaces.
pub fn tokens(s: &str) -> Vec<String> {
    let (mut r, mut cur) = (Vec::new(), String::new());
    let (mut depth, mut quote) = (0i32, false);
    for c in s.chars() {
        if c == '"' {
            quote = !quote;
        }
        if !quote {
            match c {
                '[' | '{' => depth += 1,
                ']' | '}' => depth -= 1,
                _ => {}
            }
        }
        if c.is_whitespace() && depth == 0 && !quote {
            if !cur.is_empty() {
                r.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        r.push(cur);
    }
    r
}

/// `BlockStateParser.parseForBlock` for `minecraft:name[k=v,...]` (properties optional).
pub fn parse_state(text: &str) -> BlockState {
    let (name, props) = match text.split_once('[') {
        Some((n, p)) => (n, Some(p.trim_end_matches(']'))),
        None => (text, None),
    };
    let block = Block::by_name(name).unwrap_or_else(|| panic!("unknown block {name}"));
    let props: Vec<(&str, &str)> = props
        .map(|p| p.split(',').filter_map(|kv| kv.split_once('=')).collect())
        .unwrap_or_default();
    block
        .state_from_properties(props)
        .unwrap_or_else(|| panic!("invalid properties in {text}"))
}

fn int(s: &str) -> i32 {
    s.parse().unwrap_or_else(|_| panic!("not an int: {s}"))
}

fn pos(a: &[String], i: usize) -> BlockPos {
    BlockPos::new(int(&a[i]), int(&a[i + 1]), int(&a[i + 2]))
}

// ---------------------------------------------------------------------------------------------------- the level

/// `flat` world of the oracle: bedrock at the min y, two dirt, grass (surface block at y = -61). Biomes stay the single plains.
struct FlatGenerator {
    layers: Vec<BlockState>,
}

impl FlatGenerator {
    fn default_layers() -> Self {
        Self {
            layers: vec![
                Block::BEDROCK.default_state(),
                Block::DIRT.default_state(),
                Block::DIRT.default_state(),
                Block::GRASS_BLOCK.default_state(),
            ],
        }
    }
}

impl ChunkGeneratorBackend for FlatGenerator {
    fn type_name_for_data_fixer(&self) -> Option<Identifier> {
        None
    }
    fn create_biomes(&self, _chunk: &mut ProtoChunk, _region: &WorldGenRegion) {}
    /// Same fill + heightmap priming as `FlatLevelSource.buildSurface`.
    fn build_terrain(&self, chunk: &mut ProtoChunk, _region: &WorldGenRegion) {
        let min_y = chunk.access.min_y();
        for ty in [HeightmapType::OceanFloorWg, HeightmapType::WorldSurfaceWg] {
            chunk.access.get_or_create_heightmap_unprimed(ty);
        }
        for (i, &state) in self.layers.iter().enumerate() {
            let y = min_y + i as i32;
            for x in 0..16 {
                for z in 0..16 {
                    chunk.set_block_state(BlockPos::new(x, y, z), state, None);
                    let access = &mut chunk.access;
                    let view = SectionsView {
                        level_height: access.level_height,
                        sections: &access.sections,
                        void_air_outside: true,
                    };
                    for ty in [HeightmapType::OceanFloorWg, HeightmapType::WorldSurfaceWg] {
                        let heightmap: &mut Heightmap =
                            access.heightmaps.get_mut(&ty).expect("created above");
                        heightmap.update(x, y, z, state, &view);
                    }
                }
            }
        }
    }
    fn get_base_height(&self, _x: i32, _z: i32, _ty: HeightmapType, height: LevelHeight) -> i32 {
        height.min_y + self.layers.len() as i32
    }
}

/// What `ServerLevel.tick` reads from the server.
pub struct Env {
    pub game_rules: GameRules,
    pub weather: WeatherState,
    pub difficulty: Difficulty,
    /// `time N`: `getOverworldClockTime()` (regional difficulty); the detached level has no timelines, so sky light ignores it.
    pub clock: i64,
}

impl LevelTickEnv for Env {
    fn runs_normally(&self) -> bool {
        true
    }
    fn is_entity_frozen(&self, _entity: &dyn Entity, _count_player_passengers: i32) -> bool {
        false
    }
    fn weather(&self) -> WeatherState {
        self.weather
    }
    fn set_weather(&mut self, weather: WeatherState) {
        self.weather = weather;
    }
    fn wake_up_from_sleep(&mut self) {}
    fn difficulty(&self) -> Difficulty {
        self.difficulty
    }
    fn overworld_clock_time(&self) -> i64 {
        self.clock
    }
    fn tick_scheduled_events(
        &mut self,
        _game_time: i64,
        _level: &mut mc_game::server::level::ServerLevel,
    ) {
    }
    fn broadcast_game_event_to_all(
        &mut self,
        _packet: &mc_protocol::packets::play::ClientboundGameEventPacket,
    ) {
    }
    fn set_game_time(&mut self, _game_time: i64) {}
    fn perform_prefixed_command(
        &mut self,
        _level: &mut ServerLevel,
        _source: mc_game::commands::CommandSourceStack,
        command: &str,
    ) {
        panic!(
            "block_scenario: command execution from a ticking level (command blocks) is not wired: `{command}`"
        );
    }
    fn command_rules(&self) -> mc_game::server::level::level_commands::CommandRules {
        Default::default()
    }
    fn teleport_through_portal(&mut self, _level: &mut ServerLevel, _entity: &mut dyn Entity) {}
}

struct Options_ {
    flat: bool,
    difficulty: Difficulty,
    preload: i32,
    features: mc_world::level_storage::FeatureFlagSet,
    /// `dimension`: the level every world command acts on.
    dimension: &'static str,
    /// `seed`: the world seed (loot random sequences, positional randoms of the geysers, ...).
    seed: i64,
}

/// Accepts every registry reference while decoding the vanilla dimension type JSON.
struct AllKnown;
impl mc_game::registry::codec::RegistryLookup for AllKnown {
    fn contains(&self, _: &str, _: &Identifier) -> mc_game::registry::codec::Lookup {
        mc_game::registry::codec::Lookup::Yes
    }
    fn tag_exists(&self, _: &str, _: &Identifier) -> mc_game::registry::codec::Lookup {
        mc_game::registry::codec::Lookup::Yes
    }
}

fn timeline(name: &str) -> Option<mc_game::world::attribute::Timeline> {
    let json = mc_data::datapack::read_string(&format!("data/minecraft/timeline/{name}.json"))?;
    mc_game::world::attribute::Timeline::from_json(&serde_json::from_str(&json).unwrap())
}

/// The timelines of a timeline tag (`data/minecraft/tags/timeline`), nested tags first-come like `HolderSet` expansion.
fn timelines_of_tag(tag: &str) -> Vec<mc_game::world::attribute::Timeline> {
    let json = mc_data::datapack::read_string(&format!("data/minecraft/tags/timeline/{tag}.json"))
        .unwrap_or_else(|| panic!("timeline tag {tag}"));
    let json: serde_json::Value = serde_json::from_str(&json).unwrap();
    let mut out = Vec::new();
    for v in json["values"].as_array().expect("values") {
        let v = v.as_str().expect("tag entry");
        let v = v.strip_prefix("minecraft:").unwrap_or(v);
        match v.strip_prefix('#') {
            Some(nested) => out.extend(timelines_of_tag(
                nested.strip_prefix("minecraft:").unwrap_or(nested),
            )),
            None => out.extend(timeline(v)),
        }
    }
    out
}

pub struct Scenario {
    pub level: Option<ServerLevel>,
    pub env: Env,
    pub out: Vec<String>,
    /// Completed ticks (`T n` header numbering).
    pub ticks: i64,
    watches: Vec<Vec<String>>,
    opts: Options_,
    real_biomes: bool,
    log: Arc<Mutex<Vec<BroadcastRecord>>>,
    actors: actors::Actors,
    line_no: usize,
    line: String,
    /// The `world_clock` totals the attribute timelines read (`time N` sets the overworld clock).
    clocks: Arc<mc_game::world::clock::ClockTotals>,
}

impl Scenario {
    pub fn new() -> Self {
        // VanillaLevel.boot: frozen, quiet world
        let mut game_rules = GameRules::new(FeatureFlagSet::DEFAULT_FLAGS);
        for rule in [
            "advance_time",
            "advance_weather",
            "spawn_mobs",
            "spawn_monsters",
            "spawn_patrols",
            "spawn_phantoms",
            "spawn_wandering_traders",
            "spawn_wardens",
            "raids",
        ] {
            set_rule(&mut game_rules, rule, "false");
        }
        set_rule(&mut game_rules, "random_tick_speed", "0");
        Scenario {
            clocks: Arc::default(),
            level: None,
            env: Env {
                game_rules,
                weather: WeatherState::default(),
                difficulty: Difficulty::Normal,
                clock: 0,
            },
            out: Vec::new(),
            ticks: 0,
            watches: Vec::new(),
            opts: Options_ {
                flat: true,
                difficulty: Difficulty::Normal,
                preload: 6,
                features: mc_world::level_storage::FeatureFlagSet::DEFAULT_FLAGS,
                dimension: "overworld",
                seed: 4242,
            },
            real_biomes: false,
            log: Arc::new(Mutex::new(Vec::new())),
            line_no: 0,
            actors: Default::default(),
            line: String::new(),
        }
    }

    pub fn level(&mut self) -> &mut ServerLevel {
        self.level.as_mut().expect("world not booted")
    }

    /// `VanillaLevel.boot`: the level with the preloaded chunks forced, ticking, entities loaded.
    fn world(&mut self) {
        if self.level.is_some() {
            return;
        }
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "mc-block-scenario-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let (shared, _rx) = ServerShared::new();
        let worldgen_biomes = self.real_biomes.then(actors::worldgen_biomes);
        let biomes = BiomeRegistry::new(
            worldgen_biomes
                .as_ref()
                .map_or_else(|| vec!["minecraft:plains".to_string()], |b| b.names()),
        );
        let dimension_json: serde_json::Value = serde_json::from_str(
            &mc_data::datapack::read_string(&format!(
                "data/minecraft/dimension_type/{}.json",
                self.opts.dimension
            ))
            .expect("dimension type json"),
        )
        .unwrap();
        let dimension_type = mc_game::registry::typed::dimension_type::DimensionType::from_json(
            &dimension_json,
            &mc_game::registry::codec::RegistryDecodeContext::new(&AllKnown),
        )
        .expect("dimension type");
        let timelines = match &dimension_type.timelines {
            mc_game::registry::typed::HolderSetRef::Tag(tag) => timelines_of_tag(tag.path()),
            mc_game::registry::typed::HolderSetRef::Elements(ids) => {
                ids.iter().filter_map(|id| timeline(id.path())).collect()
            }
        };
        // flat only exists in the overworld; the other dimensions are void here (scenarios fill what they dump)
        let generator: Arc<dyn ChunkGeneratorBackend> =
            if self.opts.flat && self.opts.dimension == "overworld" {
                Arc::new(FlatGenerator::default_layers())
            } else {
                Arc::new(FlatGenerator { layers: Vec::new() })
            };
        // `VanillaLevel.boot` reseeds the uniquifier before the server exists: the level's sound seed generator is deterministic.
        // The vanilla server draws unique seeds before the overworld's sound seed generator (the 5th draw, measured against the
        // oracle's first sound seed), our level one before it. The other dimensions come after further draws: not reproduced.
        mc_core::random::reseed_uniquifier(8682522807148012);
        for _ in 0..3 {
            mc_core::random::generate_unique_seed();
        }
        let mut level = ServerLevel::new(ServerLevelParams {
            shared,
            dimension: Identifier::parse(&format!("minecraft:{}", self.opts.dimension)).unwrap(),
            level_id: "block_scenario".to_string(),
            dimension_dir: dir,
            level_height: LevelHeight {
                min_y: dimension_type.min_y,
                height: dimension_type.height,
            },
            // the generator settings' sea levels (the replay has no noise settings)
            sea_level: match self.opts.dimension {
                "overworld" => 63,
                "the_nether" => 32,
                _ => 0,
            },
            has_sky_light: dimension_type.has_sky_light,
            factory: Arc::new(PalettedContainerFactory::create(biomes).expect("plains registered")),
            generator,
            generate_structures: false,
            hooks: Arc::new(mc_game::server::level::world_gen::NoWorldGenHooks),
            seed: self.opts.seed,
            is_known_structure: Arc::new(|_| false),
            view_distance: 4,
            simulation_distance: 4,
            sync_writes: false,
            is_debug: false,
            level_data: Box::new(mc_game::server::level::level_data::PrimaryServerLevelData(
                {
                    let data = mc_game::server::level::level_data::default_world_data();
                    if self.opts.flat {
                        // the vanilla flat world's spawn (natural spawning keeps its distance from it)
                        data.lock().respawn_data.pos = BlockPos::new(0, -60, 0);
                    }
                    data
                },
            )),
        });
        // dimension type attributes plus its timelines (the day timeline drives `SKY_LIGHT_LEVEL` and `SUN_ANGLE`); no biome layers
        level.tick_state.attributes = mc_game::world::attribute::EnvironmentAttributeSystem::build(
            &dimension_type.attributes,
            &vec![None],
            &timelines,
            false, // shortcut: no weather layers; the scenarios keep the weather clear, so the sky does not depend on them
            self.clocks.clone(),
        );
        // the level's own id sequence (starts at 1 like the oracle's `ENTITY_COUNTER`): parallel tests must not share ids
        level.entity_id_counter = Some(std::sync::atomic::AtomicI32::new(0));
        level.tick_state.tick_time = true;
        if let Some(biomes) = worldgen_biomes {
            level.tick_state.biomes = Some(biomes);
            level.tick_state.biome_zoom_seed =
                mc_worldgen::biome::BiomeManager::obfuscate_seed(4242);
        }
        // `Level.canHaveWeather()`
        level.tick_state.can_have_weather = dimension_type.has_sky_light
            && !dimension_type.has_ceiling
            && self.opts.dimension != "the_end";
        level.tick_state.enabled_features = self.opts.features;
        level.server_state.rules.refresh(&self.env.game_rules);
        // loot random sequences are seeded from the world seed (`VanillaLevel.Options.seed`)
        level.server_state.loot.world_seed = self.opts_seed();
        level.tick_state.broadcast_log = Some(self.log.clone());
        self.env.difficulty = self.opts.difficulty;
        let r = self.opts.preload;
        for cx in -r..=r {
            for cz in -r..=r {
                level
                    .chunk_source_mut()
                    .update_chunk_forced(ChunkPos::new(cx, cz), true);
            }
        }
        let start = Instant::now();
        loop {
            level.tick_chunk_source(&|| true, false, true);
            while level.poll_chunk_task() {}
            level.tick_entity_management();
            let ready = (-r..=r).all(|cx| {
                (-r..=r).all(|cz| {
                    level.is_position_ticking_with_entities_loaded(ChunkPos::pack_xz(cx, cz))
                })
            });
            if ready {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(120),
                "chunks did not reach entity ticking"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        use mc_core::random::RandomSource;
        level
            .tick_state
            .random
            .set_seed(self.opts_seed() ^ 0x5DEECE66D);
        self.level = Some(level);
        // loot tables, damage types, ... (the oracle server has them from the start)
        self.load_registries();
        mc_core::random::reseed_uniquifier(8682522807148012);
    }

    fn opts_seed(&self) -> i64 {
        self.opts.seed
    }

    // ------------------------------------------------------------------------------------------------ commands

    #[expect(
        clippy::print_stderr,
        reason = "test diagnostics: show the scenario context before re-raising the panic"
    )]
    fn run(&mut self, a: &[String]) {
        let line = self.line.clone();
        let ctx = format!("scenario line {}: {}", self.line_no, line);
        // `EntityOracleDriver.run`: entities a command creates (drops, spawns) draw the seeds of `hash(line)`
        if self.level.is_some() && a[0] != "reseed" {
            mc_core::random::reseed_uniquifier(super::entity_scenario::java_hash(&line));
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.run_command(a);
            self.name_new_entities();
        }));
        if let Err(e) = result {
            eprintln!("{ctx}");
            std::panic::resume_unwind(e);
        }
    }

    fn run_command(&mut self, a: &[String]) {
        use mc_core::random::RandomSource;
        if self.level.is_some() {
            // the oracle driver seeds the entity random sequence from the command line (drops, spawns draw reproducible seeds)
            mc_core::random::reseed_uniquifier(super::entity_scenario::java_hash(&self.line));
        }
        match a[0].as_str() {
            "level" => {
                assert!(
                    self.level.is_none(),
                    "`level` only before the first world command"
                );
                self.opts.flat = match a[1].as_str() {
                    "flat" => true,
                    "void" => false,
                    other => {
                        panic!("level type {other} is not supported by the Rust replay (flat|void)")
                    }
                };
            }
            "dimension" => {
                assert!(
                    self.level.is_none(),
                    "`dimension` only before the first world command"
                );
                self.opts.dimension = match a[1].as_str() {
                    "overworld" => "overworld",
                    "the_nether" => "the_nether",
                    "the_end" => "the_end",
                    other => panic!("dimension {other}"),
                };
            }
            "reseed" => {
                self.world();
                mc_core::random::reseed_uniquifier(a[1].parse().expect("reseed N"));
            }
            "seed" => self.opts.seed = a[1].parse().expect("seed N"),
            "difficulty" => {
                self.opts.difficulty = match a[1].as_str() {
                    "peaceful" => Difficulty::Peaceful,
                    "easy" => Difficulty::Easy,
                    "normal" => Difficulty::Normal,
                    "hard" => Difficulty::Hard,
                    other => panic!("difficulty {other}"),
                };
            }
            "preload" => self.opts.preload = int(&a[1]),
            "features" => {
                use mc_world::level_storage::FeatureFlagSet as F;
                for name in a[1].split(',').filter(|n| !n.is_empty()) {
                    self.opts.features = self.opts.features.join(
                        match name.strip_prefix("minecraft:").unwrap_or(name) {
                            "redstone_experiments" => F::REDSTONE_EXPERIMENTS,
                            "trade_rebalance" => F::TRADE_REBALANCE,
                            "minecart_improvements" => F::MINECART_IMPROVEMENTS,
                            other => panic!("unknown feature flag {other}"),
                        },
                    );
                }
            }
            // `cmd weather rain|clear DURATION` only (the duration is explicit: the command's default draws from the level random)
            "cmd" => {
                self.world();
                match (a.get(1).map(String::as_str), a.get(2).map(String::as_str)) {
                    (Some("weather"), Some(kind @ ("rain" | "clear"))) => {
                        let ticks: i32 = a
                            .get(3)
                            .and_then(|d| d.parse().ok())
                            .unwrap_or_else(|| panic!("weather {kind} needs a duration in ticks"));
                        self.env.weather = if kind == "rain" {
                            WeatherState {
                                clear_weather_time: 0,
                                rain_time: ticks,
                                thunder_time: ticks,
                                thundering: false,
                                raining: true,
                            }
                        } else {
                            WeatherState {
                                clear_weather_time: ticks,
                                rain_time: 0,
                                thunder_time: 0,
                                thundering: false,
                                raining: false,
                            }
                        };
                    }
                    _ => panic!("cmd {:?} is not supported by the Rust replay", &a[1..]),
                }
            }
            "note" => self.out.push(format!("# {}", self.line[4..].trim())),
            "gamerule" => {
                self.world();
                self.gamerule(&a[1], &a[2]);
            }
            "time" => {
                self.world();
                let ticks: i64 = a[1].parse().unwrap_or_else(|_| panic!("time {}", a[1]));
                self.env.clock = ticks;
                // `/time set` runs from the overworld command source: it moves the overworld clock whatever `dimension` is
                // (`modifyClock` then drops the cached attribute values); the sky follows from the next tick (`updateSkyBrightness`)
                self.clocks
                    .set(&Identifier::parse("minecraft:overworld").unwrap(), ticks);
                self.level().tick_state.attributes.invalidate_tick_cache();
            }
            "entity_counter" => {
                self.world();
                let n = int(&a[1]);
                self.level()
                    .entity_id_counter
                    .as_ref()
                    .expect("per-level counter")
                    .store(n, std::sync::atomic::Ordering::Relaxed);
            }
            "level_seed" => {
                self.world();
                let seed: i64 = a[1].parse().unwrap();
                self.level().tick_state.random.set_seed(seed);
            }
            "rand_value" => {
                self.world();
                self.level().tick_state.rand_value = int(&a[1]);
            }
            "block" => {
                self.world();
                let (p, state) = (pos(a, 1), parse_state(&a[4]));
                self.level().set_block(p, state, 3, 512);
            }
            "fill" => {
                self.world();
                let (p1, p2, state) = (pos(a, 1), pos(a, 4), parse_state(&a[7]));
                for x in p1.x().min(p2.x())..=p1.x().max(p2.x()) {
                    for y in p1.y().min(p2.y())..=p1.y().max(p2.y()) {
                        for z in p1.z().min(p2.z())..=p1.z().max(p2.z()) {
                            self.level()
                                .set_block(BlockPos::new(x, y, z), state, 3, 512);
                        }
                    }
                }
            }
            "bonemeal" => {
                self.world();
                let p = pos(a, 1);
                let grown = self.bonemeal(p);
                self.out.push(format!("BONEMEAL {grown}"));
            }
            "destroy" => {
                self.world();
                let destroyed = self.level().destroy_block_impl(pos(a, 1), true, None, 512);
                self.out.push(format!("DESTROY {destroyed}"));
            }
            "tick_chunk" => {
                self.world();
                self.level()
                    .tick_chunk(ChunkPos::new(int(&a[1]), int(&a[2])), int(&a[3]));
            }
            "random_tick" => {
                self.world();
                self.random_tick(pos(a, 1));
            }
            "be_nbt" => {
                self.world();
                let snbt = self.line[self.line.find(a[4].as_str()).expect("snbt")..].to_string();
                self.be_nbt(pos(a, 1), &snbt);
            }
            "tick" => {
                self.world();
                for _ in 0..a.get(1).map_or(1, |n| int(n)) {
                    self.tick_once();
                }
            }
            "watch" => {
                if a[1] == "off" {
                    self.watches.clear();
                } else {
                    self.watches.push(a[1..].to_vec());
                }
            }
            "dump" => {
                self.world();
                let what: Vec<String> = if a.len() > 1 {
                    a[1..].to_vec()
                } else {
                    vec!["entities".to_string(), "all_players".to_string()]
                };
                self.dump(&what);
            }
            _ if self.actor_command(a) => {}
            other => {
                panic!("command `{other}` is not supported by tests/common/block_scenario.rs yet")
            }
        }
    }

    fn gamerule(&mut self, name: &str, value: &str) {
        set_rule(&mut self.env.game_rules, name, value);
        self.level
            .as_mut()
            .expect("world")
            .server_state
            .rules
            .refresh(&self.env.game_rules);
    }

    /// One real server tick; with `watch`es set the `T n` header and the dumps follow.
    pub fn tick_once(&mut self) {
        // `VanillaLevel.tick`: every tick restarts the entity-seed sequence
        mc_core::random::reseed_uniquifier(
            8682522807148012i64 ^ ((self.ticks + 1).wrapping_mul(0x9E3779B97F4A7C15u64 as i64)),
        );
        let mut level = self.level.take().expect("world");
        level.tick(&mut self.env, &mut || true);
        self.level = Some(level);
        self.tick_players();
        self.name_new_entities();
        self.ticks += 1;
        if !self.watches.is_empty() {
            self.out.push(format!("T {}", self.ticks));
            for w in self.watches.clone() {
                self.dump(&w);
            }
        }
    }

    /// The inner loop of `ServerLevel.tickChunk` for one chosen position.
    fn random_tick(&mut self, p: BlockPos) {
        let level = self.level();
        let state = level.get_block_state(p);
        if state.is_randomly_ticking() {
            state.random_tick(level, p);
        }
        let fluid = state.get_fluid_state();
        if fluid.is_randomly_ticking() {
            fluid.random_tick(level, p);
        }
    }

    /// `BoneMealItem.growCrop(new ItemStack(BONE_MEAL, 64), level, pos)`. Saplings, azalea and mushrooms grow features:
    /// they need the data pack registries and a generation environment (the feature harness's overworld one; the grown
    /// features never consult the generator).
    fn bonemeal(&mut self, p: BlockPos) -> bool {
        self.load_registries();
        let level = self.level();
        if level.gen_env.is_none() {
            level.gen_env = Some(super::feature_harness::gen_env(4242));
        }
        let mut stack = mc_game::world::item::ItemStack::new(mc_data::Item::BONE_MEAL);
        mc_game::world::item::bone_meal_item::grow_crop(&mut stack, level, p)
    }

    /// `/data merge block`.
    fn be_nbt(&mut self, p: BlockPos, snbt: &str) {
        let tag =
            mc_nbt::snbt::parse_compound(snbt).unwrap_or_else(|e| panic!("bad SNBT {snbt}: {e}"));
        let level = self.level();
        let state = level.get_block_state(p);
        let mut merged = level
            .get_block_entity_immediate(p)
            .unwrap_or_else(|| panic!("no block entity at {}", p.to_short_string()))
            .save_with_full_metadata();
        merged.merge(&tag);
        // `loadWithComponents` on a block entity that has its level: the level-attached `loadAdditional` effects run
        assert!(level.load_block_entity_in_level(p, &merged));
        level.block_entity_set_changed(p, state); // be.setChanged() of `/data merge block`
        level.send_block_updated(p, state, state, 3);
    }

    // ------------------------------------------------------------------------------------------------ dumps

    pub fn dump(&mut self, what: &[String]) {
        for w in what {
            let (kind, arg) = w
                .split_once(':')
                .map_or((w.as_str(), None), |(k, v)| (k, Some(v)));
            match kind {
                "blocks" => {
                    let c: Vec<i32> = arg
                        .expect("blocks:x1,y1,z1,x2,y2,z2")
                        .split(',')
                        .map(int)
                        .collect();
                    assert_eq!(c.len(), 6, "blocks:x1,y1,z1,x2,y2,z2");
                    for x in c[0].min(c[3])..=c[0].max(c[3]) {
                        for y in c[1].min(c[4])..=c[1].max(c[4]) {
                            for z in c[2].min(c[5])..=c[2].max(c[5]) {
                                let state = self.level().get_block_state(BlockPos::new(x, y, z));
                                if !state.is_air() {
                                    self.out
                                        .push(format!("B {x} {y} {z} {}", state_string(state)));
                                }
                            }
                        }
                    }
                }
                "be" => {
                    let c: Vec<i32> = arg.expect("be:x,y,z").split(',').map(int).collect();
                    let p = BlockPos::new(c[0], c[1], c[2]);
                    let text = match self.level().get_block_entity(p) {
                        Some(be) => snbt(be.save_with_full_metadata()),
                        None => "-".to_string(),
                    };
                    self.out
                        .push(format!("BE {} {} {} {text}", c[0], c[1], c[2]));
                }
                "beu" => {
                    let c: Vec<i32> = arg.expect("beu:x,y,z").split(',').map(int).collect();
                    let text = match self
                        .level()
                        .get_block_entity(BlockPos::new(c[0], c[1], c[2]))
                        .and_then(|be| be.get_update_packet())
                    {
                        Some(p) => format!("{} {}", p.r#type.name(), snbt(p.tag)),
                        None => "-".to_string(),
                    };
                    self.out
                        .push(format!("BEU {} {} {} {text}", c[0], c[1], c[2]));
                }
                "bes" => {
                    let c: Vec<i32> = arg.expect("bes:cx,cz").split(',').map(int).collect();
                    self.dump_bes(c[0], c[1]);
                }
                "chunkbes" => {
                    let c: Vec<i32> = arg.expect("chunkbes:cx,cz").split(',').map(int).collect();
                    self.dump_chunk_packet_bes(c[0], c[1]);
                }
                "ticks" => self.dump_ticks(arg),
                _ if self.dump_actor(kind, arg) => {}
                "events" => self.dump_events(),
                other => panic!("dump {other} is not supported by tests/common/block_scenario.rs"),
            }
        }
    }

    /// `dump chunkbes:cx,cz`: the block entity list of the chunk's `ClientboundLevelChunkPacketData`, in packet order.
    fn dump_chunk_packet_bes(&mut self, cx: i32, cz: i32) {
        let level = self.level();
        let chunk = level.chunk_now(cx, cz).expect("chunkbes: chunk not loaded");
        let packet = mc_game::server::network::player_chunk_sender::level_chunk_with_light_packet(
            level, chunk,
        );
        let mut rows = vec![format!("CHUNKBES {cx} {cz}")];
        for info in &packet.chunk_data.block_entities_data {
            rows.push(format!(
                "BEP {} {} {} {}",
                cx * 16 + i32::from((info.packed_xz >> 4) & 15),
                info.y,
                cz * 16 + i32::from(info.packed_xz & 15),
                info.r#type.name()
            ));
        }
        self.out.extend(rows);
    }

    /// `dump bes:cx,cz`: the chunk's block entity map in iteration order, then the level's ticker lists for that chunk.
    fn dump_bes(&mut self, cx: i32, cz: i32) {
        let level = self.level();
        let mut rows = vec![format!("BES {cx} {cz}")];
        let chunk = level.chunk_now(cx, cz).expect("bes: chunk not loaded");
        for (p, be) in chunk.block_entities.iter() {
            rows.push(format!(
                "BEM {} {} {} {}",
                p.x(),
                p.y(),
                p.z(),
                be.get_type().name()
            ));
        }
        for (tag, list) in [
            ("BET", &level.tick_state.block_entity_tickers),
            ("BETP", &level.tick_state.pending_block_entity_tickers),
        ] {
            // a removed block entity's ticker is rebound to `NULL_TICKER`, whose position is `BlockPos.ZERO`
            for p in list.iter().map(|t| t.0.get().unwrap_or(BlockPos::ZERO)) {
                if p.x() >> 4 == cx && p.z() >> 4 == cz {
                    rows.push(format!("{tag} {} {} {}", p.x(), p.y(), p.z()));
                }
            }
        }
        self.out.extend(rows);
    }

    /// `ticks[:x1,y1,z1,x2,y2,z2]`: the pending ticks (inside the optional box) in execution order.
    fn dump_ticks(&mut self, area: Option<&str>) {
        let b: Vec<i32> = area.map_or_else(Vec::new, |a| a.split(',').map(int).collect());
        let inside = |p: BlockPos| {
            b.is_empty()
                || (b[0].min(b[3])..=b[0].max(b[3])).contains(&p.x())
                    && (b[1].min(b[4])..=b[1].max(b[4])).contains(&p.y())
                    && (b[2].min(b[5])..=b[2].max(b[5])).contains(&p.z())
        };
        let r = self.opts.preload;
        let game_time = self.level().game_time();
        let mut rows: Vec<(i64, i32, i64, String)> = Vec::new();
        let containers = self.level().tick_state.ticks.clone();
        let containers = containers.lock();
        for cx in -r..=r {
            for cz in -r..=r {
                let cp = ChunkPos::new(cx, cz);
                if let Some(c) = containers.block.container(cp) {
                    for t in c.get_all().filter(|t| inside(t.pos)) {
                        rows.push((
                            t.trigger_tick,
                            t.priority.get_value(),
                            t.sub_tick_order,
                            format!(
                                "TICK block {} {} {} {} delay={} prio={}",
                                t.pos.x(),
                                t.pos.y(),
                                t.pos.z(),
                                t.ty.name(),
                                t.trigger_tick - game_time,
                                t.priority.get_value()
                            ),
                        ));
                    }
                }
                if let Some(c) = containers.fluid.container(cp) {
                    for t in c.get_all().filter(|t| inside(t.pos)) {
                        rows.push((
                            t.trigger_tick,
                            t.priority.get_value(),
                            t.sub_tick_order,
                            format!(
                                "TICK fluid {} {} {} {} delay={} prio={}",
                                t.pos.x(),
                                t.pos.y(),
                                t.pos.z(),
                                t.ty.name(),
                                t.trigger_tick - game_time,
                                t.priority.get_value()
                            ),
                        ));
                    }
                }
            }
        }
        rows.sort_by_key(|(trigger, prio, sub, _)| (*trigger, *prio, *sub));
        self.out.extend(rows.into_iter().map(|r| r.3));
    }

    fn dump_events(&mut self) {
        let records = std::mem::take(&mut *self.log.lock());
        for r in records {
            self.out.push(format_broadcast(&r));
        }
    }
}

/// `/gamerule NAME VALUE` (unknown rules and rules disabled by the feature flags are ignored like a rejected command).
fn set_rule(rules: &mut GameRules, name: &str, value: &str) {
    let id = Identifier::parse(&format!("minecraft:{name}"))
        .unwrap_or_else(|_| panic!("gamerule {name}"));
    let rule = AnyGameRule::by_identifier(&id).unwrap_or_else(|| panic!("unknown gamerule {name}"));
    let parsed = rule
        .deserialize(value)
        .unwrap_or_else(|_| panic!("gamerule {name}: bad value {value}"));
    rules.try_set_value(rule, parsed);
}

/// `BlockStateParser.serialize`.
pub fn state_string(state: BlockState) -> String {
    mc_game::commands::arguments::blocks::block_state_parser::serialize(state)
}

/// `CompoundTag.toString()` (sorted keys).
pub fn snbt(tag: CompoundTag) -> String {
    mc_nbt::snbt::print_compact(&Tag::Compound(tag))
}

fn sound_source_name(s: SoundSource) -> &'static str {
    match s {
        SoundSource::Master => "master",
        SoundSource::Music => "music",
        SoundSource::Records => "record",
        SoundSource::Weather => "weather",
        SoundSource::Blocks => "block",
        SoundSource::Hostile => "hostile",
        SoundSource::Neutral => "neutral",
        SoundSource::Players => "player",
        SoundSource::Ambient => "ambient",
        SoundSource::Voice => "voice",
        SoundSource::Ui => "ui",
    }
}

/// The `EV ...` line of a recorded broadcast (twin of `VanillaLevel.RecordingPlayerList`).
pub fn format_broadcast(r: &BroadcastRecord) -> String {
    let head = format!("r={:?} ex={}", r.range, i32::from(r.except));
    let mut body: &[u8] = &r.payload;
    let id = read_varint(&mut body).expect("packet id");
    if id == ClientboundSoundPacket::ID {
        let p = ClientboundSoundPacket::decode(&mut body).expect("sound packet");
        let event = match &p.sound {
            SoundEventHolder::Reference(s) => s.name().to_string(),
            SoundEventHolder::Direct(_) => "[unregistered]".to_string(),
        };
        format!(
            "EV sound at={:?},{:?},{:?} {head} event={event} source={} vol={:?} pitch={:?}",
            r.x,
            r.y,
            r.z,
            sound_source_name(p.source),
            p.volume,
            p.pitch
        )
    } else if id == ClientboundLevelEventPacket::ID {
        let p = ClientboundLevelEventPacket::decode(&mut body).expect("level event packet");
        format!(
            "EV level_event {head} type={} pos={},{},{} data={} global={}",
            p.r#type,
            p.pos.x(),
            p.pos.y(),
            p.pos.z(),
            p.data,
            i32::from(p.global_event)
        )
    } else if id == ClientboundBlockEventPacket::ID {
        let p = ClientboundBlockEventPacket::decode(&mut body).expect("block event packet");
        format!(
            "EV block_event {head} pos={},{},{} block={} b0={} b1={}",
            p.pos.x(),
            p.pos.y(),
            p.pos.z(),
            p.block.name(),
            p.b0,
            p.b1
        )
    } else {
        let name = mc_protocol::packet::packet_name(
            mc_protocol::packet::ConnectionState::Play,
            mc_protocol::packet::PacketFlow::Clientbound,
            id,
        )
        .unwrap_or("?");
        format!(
            "EV packet at={:?},{:?},{:?} {head} id={name}",
            r.x, r.y, r.z
        )
    }
}
