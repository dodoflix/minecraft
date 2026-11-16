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
//! Supported commands (same text as the Java driver, see its header): `level flat|void|normal` (`normal`: the real noise generator of the dimension), `structures true|false` (`generate-structures`: placed structures and the structure search), `dimension overworld|the_nether|the_end`, `seed`, `difficulty`, `preload R`, `preload_settle` (boot also finishes the chunks two rings beyond the preload), `features NAME,..`,
//! `command_env` (a real command environment around the whole scenario; the command verbs of `command_verbs.rs` then run real commands: `cmd COMMAND...` as the console with its feedback printed as compared `CF` lines, `cmd_quiet`, `cmd_json`, `cmd_as PLAYER|-LEVEL COMMAND...` with a player's or a permission level's permissions; dumps `scoreboard`, `bossbars`, `storage:ID`, `select:EXECUTOR/X,Y,Z/SELECTOR`), `jigsaw_generate PLAYER X Y Z LEVELS KEEP`, `menu_button PLAYER N`, `close_menu PLAYER`, `move_packet`, `move_vehicle`, `paddle`, `client_tick_end`, `respawn`, `effect`, `mobspawn` (the actor verbs of `block_scenario_actors.rs`), the mob verbs of `mob_verbs.rs` (`target love age breed tame leash mob_attack nav path_query nav_stop`, dumps `mob:REF mobs goals: brain: path: attrs: nbt:`),
//! `note`, the spawning verbs of `spawn_verbs.rs` (`spawnpass`, `spawnat`, `custom_spawner`, `raid_start`, `raid_waves`, `rest`, `memory`, `activity`, dumps `difficulty:X,Y,Z spawn_state raids`), `gamerule NAME VALUE`, `time N` (the overworld clock: sky light and sun angle follow from the next tick), `block X Y Z STATE`, `fill X1 Y1 Z1 X2 Y2 Z2 STATE`, `level_seed N`, `entity_counter N` (the next entity gets id N+1; the level's own counter starts at 0), `rand_value N`,
//! `tick_chunk CX CZ SPEED`, `random_tick X Y Z`, `be_nbt X Y Z SNBT`, `bonemeal X Y Z` (`BONEMEAL true|false`), `biome X1 Y1 Z1 X2 Y2 Z2 BIOME` (`/fillbiome`: loads the real biome registry), `destroy X Y Z` (`Level.destroyBlock`, `DESTROY true|false`), `tick [N]`, `watch WHAT..`, `dump WHAT..` with WHAT in
//! `blocks:x1,y1,z1,x2,y2,z2` (non-air blocks as `B x y z state`), `be:x,y,z` (`BE x y z SNBT`), `beu:x,y,z` (`BEU x y z TYPE SNBT` = update packet tag), `bes:cx,cz` (`BES`/`BEM x y z type`/`BET x y z`/`BETP x y z`: block entity map and ticker list), `chunkbes:cx,cz` (`CHUNKBES`/`BEP x y z type`: the block entity list of the chunk packet; `BEM` is the map's iteration order, vanilla's fastutil order), `ticks[:x1,y1,z1,x2,y2,z2]` (`TICK block|fluid x y z id
//! delay=N prio=P`, execution order), `events` (`EV ...` lines of level events / sounds / block events broadcast since the last
//! dump: [`ServerLevel`]'s `tick_state.broadcast_log` here, `VanillaLevel.RecordingPlayerList` in Java). Everything else panics
//! with the line number, so a scenario never silently runs half.
//! Persistence: `save [flush]`, `unload CX CZ`, `unloadall`, `load CX CZ`, `reload CX CZ`, `rejoin PLAYER`, dumps `region:CX,CZ`, `entityfile:CX,CZ`, `poifile:CX,CZ`, `playerfile:PLAYER`, `datafile:ID` (`block_scenario_persist.rs`).
//! Multipart entities: `partref NAME DRAGON INDEX` (the hitbox INDEX of an ender dragon as a REF), dumps `parts:REF` (`PART ..` per hitbox) and `counter` (`COUNTER n`). `crystal_destroyed DRAGON CRYSTAL DAMAGETYPE [by=..]` calls the dragon's crystal reaction directly.
//!
//! Notes: the level ticks like `ServerLevel::tick` (scheduled block/fluid ticks, block events, random ticks, block entity
//! tickers; `random_tick_speed` defaults to 0 like the oracle), game time advances one per tick. Prefer `tick_chunk` + `rand_value` to
//! `gamerule random_tick_speed`: whole-level random ticking depends on chunk iteration order, which the two servers do not share.
//! `gamerule` goes through the real `GameRules` (all rules).
#[path = "block_scenario_actors.rs"]
mod actors;
#[path = "block_scenario_fight.rs"]
mod fight;
#[path = "block_scenario_persist.rs"]
mod persist;
use super::entity_oracle::{Fixture, Options, compare, fixture_dir, scenario_dir};
use mc_core::ident::Identifier;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::block::{Block, BlockState};
use mc_game::server::level::level_events::BroadcastRecord;
use mc_game::server::level::level_tick::{LevelTickEnv, WeatherState};
use mc_game::server::level::world_gen::{ChunkGeneratorBackend, WorldGenRegion};
use mc_game::server::level::{ServerLevel, ServerLevelParams};
use mc_game::server::shared::ServerShared;
use mc_game::world::entity::{Entity, EntityId};
use mc_game::world::level::block::BlockStateExt;
use mc_game::world::level::gamerules::{AnyGameRule, GameRules};
use mc_game::world::level::material::FluidStateExt;
use mc_game::world::level::{BlockGetter, LevelAccessor, LevelReader, LightLayer};
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

/// [`check`] of the lines before the first `# MARKER` note: the leading part of a scenario, checked on its own ahead of a later
/// section that is also covered by a full [`check`].
pub fn check_until(name: &str, marker: &str) {
    let actual = run_scenario(name);
    let marker = format!("# {marker}");
    let cut = |lines: &[String]| -> Vec<String> {
        lines
            .iter()
            .take_while(|l| **l != marker)
            .cloned()
            .collect()
    };
    let fixture = Fixture::load(name);
    let mut opts = Options::default();
    opts.ignore_containing.push("minecraft:light_update");
    if let Err(report) = compare(&cut(&fixture.lines), &cut(&actual), &opts) {
        panic!("block oracle scenario `{name}` differs before `{marker}`:\n{report}");
    }
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
    let mut opts = opts.clone();
    // light updates come from the light thread: which tick carries them depends on thread timing in vanilla
    opts.ignore_containing.push("minecraft:light_update");
    if let Err(report) = compare(&fixture.lines, &actual, &opts) {
        panic!(
            "block oracle scenario `{name}` differs from {}:\n{report}",
            fixture_dir().join(format!("{name}.txt")).display()
        );
    }
}

pub fn run_scenario_text(text: &str) -> Vec<String> {
    actors::reset_login_detail();
    mc_game::world::level::material::flowing_fluid::reset_shape_memo_for_scenario();
    // `dump console` reads the log: the capture must be installed before the first line that logs (a player joining)
    if text
        .lines()
        .any(|l| l.starts_with("dump ") && l.split_whitespace().any(|w| w == "console"))
    {
        super::init_logger();
    }
    let mut s = Scenario::new();
    // precipitation, the bone meal feature lists, the coral bone meal tag and the biome criteria of advancements (adventuring time) read
    // the biome (natural spawning reads the spawn table of the biome): only the weather, `biome`, spawning, bone meal and advancement scenarios pay for the real biome registry
    s.real_biomes = text.lines().any(|l| {
        let l = l.trim_start();
        l.starts_with("cmd weather")
            || l.contains("fillbiome ")
            || l.contains(" if biome ")
            || l.contains("locate biome ")
            || l.starts_with("bonemeal")
            || l.starts_with("biome ")
            || l.contains(" fillbiome ")
            || l.starts_with("spawnpass ")
            || l.starts_with("spawnat ")
            || l.starts_with("gamerule spawn_mobs true")
            || l.starts_with("custom_spawner ")
            || l.contains("if biome ")
            || l.contains("unless biome ")
            || l.contains("minecraft:bone_meal")
            || l.contains("advancements:")
            || l.contains("advancementsfile:")
    });
    // the generation environment also holds the block transformer registry (axes, shovels and hoes strip, scrape, flatten and till)
    s.gen_env = text.contains("minecraft:structure_block")
        || text.contains("jigsaw_generate")
        || ["_axe", "_shovel", "_hoe"].iter().any(|t| text.contains(t));
    // `/place structure` on a flat level generates with the flat chunk generator (the first free height of the layers), not the noise one
    s.flat_structures = text.contains("place structure") || text.contains("locate ");
    // `command_env`: the vanilla server always has its command environment (lecterns resolve written book pages through it);
    // the replay installs a real `MinecraftServer` (nothing started) around every command of the scenario. The verbs that
    // deliver a packet to the player's listener (`select`, `close_menu`, `menu_button`, `pick_block`) need it as well.
    // `datapack NAME`: the data packs `tests/oracle/datapacks/NAME` the server loads (the oracle copies them into its world)
    let packs: Vec<String> = text
        .lines()
        .filter_map(|l| l.trim().strip_prefix("datapack "))
        .map(|name| name.trim().to_owned())
        .collect();
    let features: Vec<String> = text
        .lines()
        .filter_map(|l| l.trim().strip_prefix("features "))
        .flat_map(|names| names.split(','))
        .map(|name| name.trim().trim_start_matches("minecraft:").to_owned())
        .filter(|name| !name.is_empty())
        .collect();
    if text.lines().any(|l| l.trim() == "dump logs") {
        super::init_logger();
    }
    let mut server = text
        .lines()
        .any(|l| {
            matches!(
                l.split_whitespace().next(),
                Some(
                    "biome"
                        | "chunk_batch_received"
                        | "command_env"
                        | "datapack"
                        | "debug_subscribe"
                        | "difficulty"
                        | "gamerule"
                        | "select"
                        | "close_menu"
                        | "menu_button"
                        | "pick_block"
                        | "player_loaded"
                        | "move_packet"
                        | "move_vehicle"
                        | "accept_teleport"
                        | "teleport_to_entity"
                        | "paddle"
                        | "client_tick_end"
                        | "levels"
                        | "leave"
                        | "rejoin"
                        | "request_stats"
                        | "respawn"
                        | "set_command_block"
                        | "set_command_minecart"
                        | "time"
                )
            )
        })
        .then(|| super::command_verbs::install_command_env(&packs, &features));
    if let Some((server, _)) = &server
        && !packs.is_empty()
    {
        s.actors.use_server_data(server);
    }
    // with a command environment the level ticks through the installed server (tick rate manager, clocks, weather, difficulty), and the
    // clocks the sky reads are the server's
    if let Some((server, _)) = &server {
        s.clocks = server.clock_manager().totals();
        s.env.clocks = s.clocks.clone();
    }
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        s.line_no = i + 1;
        s.line = line.to_string();
        match server.as_mut() {
            Some((server, _keep)) => mc_game::commands::server_access::enter_server(server, || {
                s.run(&tokens(line));
                s.run_pending_functions();
            }),
            None => s.run(&tokens(line)),
        }
    }
    s.out
        .iter()
        .map(|l| super::command_verbs::normalize_clock_stamps(l))
        .collect()
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

/// The key of the dimension `overworld`, `the_nether` or `the_end`.
fn dimension_key(dimension: &str) -> mc_game::world::level::DimensionKey {
    Identifier::parse(&format!("minecraft:{dimension}")).expect("dimension key")
}

/// The replay's name of a level's dimension key.
fn static_dimension(key: &mc_game::world::level::DimensionKey) -> &'static str {
    match key.path() {
        "overworld" => "overworld",
        "the_nether" => "the_nether",
        "the_end" => "the_end",
        other => panic!("dimension {other} is not replayed"),
    }
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
    /// `FlatLevelSource.getSpawnHeight`.
    fn get_spawn_height(&self, height: LevelHeight) -> i32 {
        height.min_y + height.height.min(self.layers.len() as i32)
    }
}

/// What `ServerLevel.tick` reads from the server.
pub struct Env {
    pub game_rules: GameRules,
    pub weather: WeatherState,
    pub difficulty: Difficulty,
    /// `time N`: `getOverworldClockTime()` (regional difficulty); the detached level has no timelines, so sky light ignores it.
    pub clock: i64,
    /// The `world_clock` totals the attribute timelines read, moved with `clock`.
    pub clocks: Arc<mc_game::world::clock::ClockTotals>,
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
    /// `moveToTimeMarker(overworld, WAKE_UP_FROM_SLEEP)`: the marker is tick 0 of every 24000-tick day.
    fn wake_up_from_sleep(&mut self) {
        self.clock += (24000 - self.clock.rem_euclid(24000)) % 24000;
        self.clocks.set(
            &Identifier::parse("minecraft:overworld").unwrap(),
            self.clock,
        );
    }
    fn difficulty(&self) -> Difficulty {
        self.difficulty
    }
    fn overworld_clock_time(&self) -> i64 {
        self.clock
    }
    fn tick_scheduled_events(
        &mut self,
        game_time: i64,
        level: &mut mc_game::server::level::ServerLevel,
    ) {
        // `/schedule function` events of the installed server's timer queue
        if mc_game::commands::server_access::has_server() {
            mc_game::commands::server_access::with_server(|server| {
                server.tick_scheduled_events(game_time, level)
            });
        }
    }
    fn broadcast_game_event_to_all(
        &mut self,
        packet: &mc_protocol::packets::play::ClientboundGameEventPacket,
    ) {
        // a replay with a command environment has a player list: the weather events reach its players
        if mc_game::commands::server_access::has_server() {
            mc_game::commands::server_access::with_server(|server| {
                server.player_list.broadcast_all(packet)
            });
        }
    }
    fn set_game_time(&mut self, _game_time: i64) {}
    fn perform_prefixed_command(
        &mut self,
        level: &mut ServerLevel,
        source: mc_game::commands::CommandSourceStack,
        command: &str,
    ) {
        // `command_env` scenarios run the command through the installed server like `LevelTickEnv` does
        assert!(
            mc_game::commands::server_access::has_server(),
            "block_scenario: command execution from a ticking level (command blocks) needs the `command_env` verb: `{command}`"
        );
        mc_game::commands::server_access::with_server(|server| {
            let commands = server.commands.clone();
            server.with_lent_level(level, |server| {
                commands.perform_prefixed_command(server, source, command)
            });
        });
    }
    fn run_pending_functions(&mut self, level: &mut ServerLevel) {
        if mc_game::commands::server_access::has_server() {
            mc_game::commands::server_access::with_server(|server| {
                server.run_pending_functions(level)
            });
        }
    }
    fn teleport_through_portal(&mut self, _level: &mut ServerLevel, _entity: &mut dyn Entity) {}
    fn run_server_actions(&mut self, level: &mut ServerLevel) {
        if mc_game::commands::server_access::has_server() {
            mc_game::commands::server_access::with_server(|server| {
                server.run_server_actions(level)
            });
        }
    }
}

/// [`Env`] of a scenario with a command environment: what `ServerTickEnv` reads from the server (tick rate manager, weather, difficulty, clocks) comes from
/// the installed server, so the commands that change them (`/tick`, `/weather`, `/difficulty`, `/time`) act on the level's tick; the rest
/// stays the replay's.
struct ServerBackedEnv<'a>(&'a mut Env);

impl LevelTickEnv for ServerBackedEnv<'_> {
    fn runs_normally(&self) -> bool {
        mc_game::commands::server_access::with_server(|server| {
            server.tick_rate_manager.runs_normally()
        })
    }
    fn is_entity_frozen(&self, entity: &dyn Entity, count_player_passengers: i32) -> bool {
        mc_game::commands::server_access::with_server(|server| {
            server
                .tick_rate_manager
                .is_entity_frozen(entity.is_player(), count_player_passengers)
        })
    }
    fn weather(&self) -> WeatherState {
        mc_game::commands::server_access::with_server(|server| {
            let weather = server.weather_data_ref();
            WeatherState {
                clear_weather_time: weather.get_clear_weather_time(),
                thunder_time: weather.get_thunder_time(),
                rain_time: weather.get_rain_time(),
                thundering: weather.is_thundering(),
                raining: weather.is_raining(),
            }
        })
    }
    fn set_weather(&mut self, weather: WeatherState) {
        mc_game::commands::server_access::with_server(|server| {
            let data = server.weather_data();
            data.set_clear_weather_time(weather.clear_weather_time);
            data.set_thunder_time(weather.thunder_time);
            data.set_rain_time(weather.rain_time);
            data.set_thundering(weather.thundering);
            data.set_raining(weather.raining);
        })
    }
    fn wake_up_from_sleep(&mut self) {
        mc_game::commands::server_access::with_server(|server| {
            server.wake_up_from_sleep(&Identifier::parse("minecraft:overworld").unwrap())
        })
    }
    fn difficulty(&self) -> Difficulty {
        mc_game::commands::server_access::with_server(|server| {
            server
                .world_data
                .lock()
                .settings
                .difficulty_settings
                .difficulty
        })
    }
    fn overworld_clock_time(&self) -> i64 {
        mc_game::commands::server_access::with_server(|server| server.overworld_clock_time())
    }
    fn tick_scheduled_events(&mut self, game_time: i64, level: &mut ServerLevel) {
        self.0.tick_scheduled_events(game_time, level)
    }
    fn broadcast_game_event_to_all(
        &mut self,
        packet: &mc_protocol::packets::play::ClientboundGameEventPacket,
    ) {
        mc_game::commands::server_access::with_server(|server| {
            server.player_list.broadcast_all(packet)
        })
    }
    fn set_game_time(&mut self, game_time: i64) {
        mc_game::commands::server_access::with_server(|server| {
            server.world_data.lock().game_time = game_time
        })
    }
    fn perform_prefixed_command(
        &mut self,
        level: &mut ServerLevel,
        source: mc_game::commands::CommandSourceStack,
        command: &str,
    ) {
        self.0.perform_prefixed_command(level, source, command)
    }
    fn run_pending_functions(&mut self, level: &mut ServerLevel) {
        self.0.run_pending_functions(level)
    }
    fn teleport_through_portal(&mut self, level: &mut ServerLevel, entity: &mut dyn Entity) {
        // the level being ticked is out of the server, like `ServerTickEnv` in the server's own tick
        mc_game::commands::server_access::with_server(|server| {
            mc_game::server::players::dimension_change::teleport_through_portal(
                server, level, entity,
            )
        });
    }
    fn run_server_actions(&mut self, level: &mut ServerLevel) {
        mc_game::commands::server_access::with_server(|server| server.run_server_actions(level));
    }
}

struct Options_ {
    flat: bool,
    /// `level normal`: the real noise generator of the dimension.
    normal: bool,
    /// `structures true`: `generate-structures` (placed structures and the structure search).
    structures: bool,
    /// `world_spawn`: the world spawn is searched like `MinecraftServer.setInitialSpawn` does (the oracle server did it at boot).
    find_world_spawn: bool,
    difficulty: Difficulty,
    /// `hardcore`: `hardcore=true` (the difficulty is hard, a respawn makes a spectator).
    hardcore: bool,
    preload: i32,
    /// `preload_settle`: boot also waits until the chunks two rings beyond the preload are full.
    settle_preload_ring: bool,
    features: mc_world::level_storage::FeatureFlagSet,
    /// `dimension`: the level every world command acts on.
    dimension: &'static str,
    /// `seed`: the world seed (loot random sequences, positional randoms of the geysers, ...).
    seed: i64,
    /// `bootrule NAME VALUE`: game rules set before the preload (chunk generation reads them).
    boot_rules: Vec<(String, String)>,
    /// `levels A,B`: the levels the replay holds besides `dimension` (empty: one detached level).
    levels: Vec<&'static str>,
    /// `end void`, `nether void`: that dimension keeps the void generator under `level normal`.
    void_dims: Vec<&'static str>,
    /// `end fight`: the End level runs its dragon fight (without it the level has none, like the oracle's after boot).
    end_fight: bool,
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

/// The vanilla dimension type JSON of `dimension` (`overworld`, `the_nether`, `the_end`).
/// The registries of the oracle's server (see `block_scenario_actors.rs`).
pub(super) fn real_registries() -> mc_game::registry::RegistryAccess {
    actors::real_registries()
}

pub(super) fn load_dimension_type(
    dimension: &str,
) -> mc_game::registry::typed::dimension_type::DimensionType {
    let dimension_json: serde_json::Value = serde_json::from_str(
        &mc_data::datapack::read_string(&format!("data/minecraft/dimension_type/{dimension}.json"))
            .expect("dimension type json"),
    )
    .unwrap();
    mc_game::registry::typed::dimension_type::DimensionType::from_json(
        &dimension_json,
        &mc_game::registry::codec::RegistryDecodeContext::new(&AllKnown),
    )
    .expect("dimension type")
}

/// The dimension type attributes plus its timelines (the day timeline drives `SKY_LIGHT_LEVEL` and `SUN_ANGLE`), the weather layers and the
/// attributes of the biomes of the real registry (none while the level only knows plains).
pub(super) fn environment_attributes(
    dimension_type: &mc_game::registry::typed::dimension_type::DimensionType,
    biomes: Option<&mc_worldgen::biome::BiomeRegistry>,
    clocks: Arc<mc_game::world::clock::ClockTotals>,
) -> mc_game::world::attribute::EnvironmentAttributeSystem {
    let mut biome_attributes: Vec<Option<&serde_json::Value>> =
        vec![None; biomes.map_or(1, |b| b.len())];
    for id in biomes.iter().flat_map(|b| b.ids()) {
        biome_attributes[id.0 as usize] = biomes.and_then(|b| b.get(id).attributes.as_ref());
    }
    let timelines = match &dimension_type.timelines {
        mc_game::registry::typed::HolderSetRef::Tag(tag) => timelines_of_tag(tag.path()),
        mc_game::registry::typed::HolderSetRef::Elements(ids) => {
            ids.iter().filter_map(|id| timeline(id.path())).collect()
        }
    };
    mc_game::world::attribute::EnvironmentAttributeSystem::build(
        &dimension_type.attributes,
        &biome_attributes,
        &timelines,
        true,
        clocks,
    )
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
    /// The world data of the level (`/difficulty` changes it).
    world_data: Option<mc_game::server::level::level_data::SharedWorldData>,
    pub env: Env,
    pub out: Vec<String>,
    /// Completed ticks (`T n` header numbering).
    pub ticks: i64,
    watches: Vec<Vec<String>>,
    opts: Options_,
    real_biomes: bool,
    /// Structure blocks and jigsaw generation read the generation environment (template manager, pool registries).
    gen_env: bool,
    /// `/place structure` is used: a flat level then carries the flat generator's environment.
    flat_structures: bool,
    log: Arc<Mutex<Vec<BroadcastRecord>>>,
    actors: actors::Actors,
    line_no: usize,
    line: String,
    /// The `world_clock` totals the attribute timelines read (`time N` sets the overworld clock).
    clocks: Arc<mc_game::world::clock::ClockTotals>,
    /// The dimensions of a `levels` replay in the order of the oracle's server (empty: one detached level).
    dims: Vec<&'static str>,
    /// The level world verbs act on (the driver's `v.level`): `dimension`, and the `in DIM` level for the length of that line.
    current: &'static str,
    /// The dimension of the level in `level`; a `levels` replay keeps every other level in the installed server.
    at: &'static str,
    /// The entity id sequence all levels of the replay share.
    entity_counter: Option<Arc<std::sync::atomic::AtomicI32>>,
}

impl Scenario {
    pub fn new() -> Self {
        let game_rules = quiet_game_rules(FeatureFlagSet::DEFAULT_FLAGS);
        let clocks = Arc::<mc_game::world::clock::ClockTotals>::default();
        Scenario {
            clocks: clocks.clone(),
            level: None,
            world_data: None,
            env: Env {
                game_rules,
                weather: WeatherState::default(),
                difficulty: Difficulty::Normal,
                clock: 0,
                clocks,
            },
            out: Vec::new(),
            ticks: 0,
            watches: Vec::new(),
            opts: Options_ {
                flat: true,
                normal: false,
                structures: false,
                find_world_spawn: false,
                difficulty: Difficulty::Normal,
                hardcore: false,
                preload: 6,
                settle_preload_ring: false,
                features: mc_world::level_storage::FeatureFlagSet::DEFAULT_FLAGS,
                dimension: "overworld",
                seed: 4242,
                boot_rules: Vec::new(),
                levels: Vec::new(),
                void_dims: Vec::new(),
                end_fight: false,
            },
            real_biomes: false,
            gen_env: false,
            flat_structures: false,
            log: Arc::new(Mutex::new(Vec::new())),
            line_no: 0,
            actors: Default::default(),
            line: String::new(),
            dims: Vec::new(),
            current: "overworld",
            at: "overworld",
            entity_counter: None,
        }
    }

    pub fn level(&mut self) -> &mut ServerLevel {
        self.level.as_mut().expect("world not booted")
    }

    /// Whether the replay holds several levels (`levels`).
    fn multi(&self) -> bool {
        !self.dims.is_empty()
    }

    /// Makes the level of `dimension` the one `level` holds: the level it held goes back into the installed server (keeping its place
    /// in the level order), so `execute in`, `/tp` and the player list reach every level while one of them is lent to a command.
    fn focus(&mut self, dimension: &'static str) {
        if self.at == dimension {
            return;
        }
        let held = self.level.take().expect("world not booted");
        self.level = mc_game::commands::server_access::with_server(|server| {
            server.add_level(held);
            server.take_level(&dimension_key(dimension))
        });
        assert!(
            self.level.is_some(),
            "no level `{dimension}` in this replay (`levels`)"
        );
        self.at = dimension;
    }

    /// The dimension of the level the entity `id` is in (`None` without `levels`, or an entity that is in no level).
    fn dimension_of(&self, id: EntityId) -> Option<&'static str> {
        if !self.multi() {
            return None;
        }
        // a player that died waits for its respawn out of the entity storage
        let live = |l: &ServerLevel| l.get_entity(id).is_some() || l.get_dead_player(id).is_some();
        if self.level.as_ref().is_some_and(live) {
            return Some(self.at);
        }
        mc_game::commands::server_access::with_server(|server| {
            server
                .all_levels()
                .find(|l| live(l))
                .map(|l| static_dimension(&l.dimension))
                // a player that watched the credits waits outside every level for its respawn request
                .or_else(|| {
                    server
                        .detached_players
                        .values()
                        .find(|p| p.base_id() == id)
                        .map(|p| static_dimension(&p.dimension))
                })
        })
    }

    /// The level the script name `name` (a player or an entity) lives in: the driver's player verbs act through the player's own
    /// connection, so a line that names an entity runs in its level.
    fn focus_ref(&mut self, name: &str) {
        let id = self
            .actors
            .refs
            .iter()
            .find(|(n, _)| n == name)
            .map(|r| r.1);
        if let Some(dimension) = id.and_then(|id| self.dimension_of(id)) {
            self.focus(dimension);
        }
    }

    /// Runs `f` on every level of the replay in server order (the one level without `levels`); the level held before is held again.
    fn for_each_level(&mut self, mut f: impl FnMut(&mut ServerLevel)) {
        if !self.multi() {
            return f(self.level());
        }
        let keep = self.at;
        for dimension in self.dims.clone() {
            self.focus(dimension);
            f(self.level());
        }
        self.focus(keep);
    }

    /// `ActionHost::command_done`: what a command changes outside the installed server.
    fn after_command(&mut self, command: &str, succeeded: bool) {
        // the level shares the installed server's rule handle: only the chunk source's spawn flag follows a `/gamerule`
        if succeeded
            && matches!(
                command.split_whitespace().next(),
                Some("difficulty" | "gamerule")
            )
        {
            self.for_each_level(apply_spawn_settings);
        }
        // a reload (or toggling a pack) swaps the server's registries, recipes and advancements: the replay's entity creation reads the new ones
        if succeeded
            && matches!(
                command.split_whitespace().next(),
                Some("reload" | "datapack")
            )
        {
            mc_game::commands::server_access::with_server(|server| {
                self.actors.refresh_server_data(server)
            });
        }
        self.handle_disconnections();
    }

    /// The reward functions queued by the line that just ran (vanilla runs them inside the award; see `PendingFunction`).
    fn run_pending_functions(&mut self) {
        if let Some(level) = self.level.as_mut()
            && !level.tick_state.pending_functions.is_empty()
        {
            mc_game::commands::server_access::with_server(|server| {
                server.run_pending_functions(level)
            });
        }
        // the levels the server holds run theirs too
        if self.multi() {
            mc_game::commands::server_access::with_server(|server| server.run_reward_functions());
        }
    }

    /// `VanillaLevel.boot`: the levels (`levels A,B`: all of them, otherwise the one `dimension`) with the preloaded chunks forced,
    /// ticking, entities loaded. A `levels` scenario hands every level to the installed server except the current one, which
    /// the scenario holds (see [`Scenario::focus`]).
    fn world(&mut self) {
        if self.level.is_some() {
            return;
        }
        let (shared, _rx) = ServerShared::new();
        // the scenario thread is the server thread: tasks the game submits (point of interest updates) run inline like in vanilla
        shared.bind_game_thread();
        self.dims = self.level_dims();
        // the oracle's server creates the overworld first, then the End, then the Nether (see `make_level`)
        let dimensions = if self.dims.is_empty() {
            vec![self.opts.dimension]
        } else {
            self.dims.clone()
        };
        let mut levels: Vec<ServerLevel> = dimensions
            .into_iter()
            .map(|dimension| self.make_level(dimension, &shared))
            .collect();
        self.current = self.opts.dimension;
        self.at = self.current;
        if self.dims.is_empty() {
            self.level = levels.pop();
        } else {
            // one player roster, entity directory and POI directory for every level, like the server's
            let roster = levels[0].server_state.roster.clone();
            let entities = levels[0].server_state.entities.clone();
            let pois = levels[0].server_state.pois.clone();
            for level in &mut levels {
                level.server_state.roster = roster.clone();
                level.server_state.entities = entities.clone();
                level.server_state.pois = pois.clone();
                pois.register(level.dimension.clone(), level.poi_manager().clone());
            }
            mc_game::commands::server_access::with_server(|server| {
                for level in levels {
                    server.add_level(level);
                }
                self.level = server.take_level(&dimension_key(self.current));
            });
            assert!(
                self.level.is_some(),
                "the `dimension` level is a `levels` one"
            );
        }
        // loot tables, damage types, ... (the oracle server has them from the start)
        self.load_registries();
        mc_core::random::reseed_uniquifier(8682522807148012);
    }

    /// The `levels` of the scenario in the order the oracle's server creates (and ticks) them; empty without a `levels` line.
    fn level_dims(&self) -> Vec<&'static str> {
        if self.opts.levels.is_empty() {
            return Vec::new();
        }
        assert!(
            mc_game::commands::server_access::has_server(),
            "`levels` needs the `command_env` verb"
        );
        ["overworld", "the_end", "the_nether"]
            .into_iter()
            .filter(|d| *d == self.opts.dimension || self.opts.levels.contains(d))
            .collect()
    }

    /// One level of `VanillaLevel.boot` for `dimension`.
    fn make_level(&mut self, dimension: &'static str, shared: &Arc<ServerShared>) -> ServerLevel {
        let shared = shared.clone();
        let scratch = tempfile::Builder::new()
            .prefix("mc-block-scenario-")
            .tempdir()
            .expect("create scratch dir");
        let noise_world =
            (self.opts.normal && !self.opts.void_dims.contains(&dimension)).then(|| {
                super::feature_harness::noise_world(self.opts.seed, self.opts.structures, dimension)
            });
        let worldgen_biomes = if noise_world.is_some() {
            Some(super::feature_harness::WORLD.biomes.clone())
        } else {
            self.real_biomes.then(actors::worldgen_biomes)
        };
        let biomes = BiomeRegistry::new(
            worldgen_biomes
                .as_ref()
                .map_or_else(|| vec!["minecraft:plains".to_string()], |b| b.names()),
        );
        let dimension_type = load_dimension_type(dimension);
        // flat only exists in the overworld; the other dimensions are void here (scenarios fill what they dump)
        let generator: Arc<dyn ChunkGeneratorBackend> = if let Some((_, backend)) = &noise_world {
            backend.clone()
        } else if self.opts.flat && dimension == "overworld" {
            Arc::new(FlatGenerator::default_layers())
        } else {
            Arc::new(FlatGenerator { layers: Vec::new() })
        };
        // `VanillaLevel.boot` reseeds the uniquifier before the server exists: the level's sound seed generator is deterministic.
        // The vanilla server creates the overworld first, then the other dimensions in the iteration order of its resource key
        // keyed registry map (the oracle's shadow `ResourceKey.hashCode` fixes it to the end, then the nether; a real server's
        // order changes per run). Every `Level` constructor draws unique seeds (`random`, the sound seed generator, ...): measured
        // against the first sound seed of each dimension in the oracle (flat and normal worlds), the sound seed generator is the 5th (overworld), 18th (end)
        // and 21st (nether) draw (the oracle runs worker tasks on the main thread, so their draws count here). `ServerLevel::new` draws `randValue` (the thread-local instance) and the level's `random` itself before the sound seed generator.
        // Every `Level` constructor draws unique seeds, so each level starts from the sequence position the oracle had when it created
        // that level (measured, see above). With the end next to the nether (`levels` naming both), the oracle's nether generator is the
        // 180th draw: the chunk tasks of the three levels drew in between.
        mc_core::random::reseed_uniquifier(8682522807148012);
        // A void overworld (`level void`) makes the server draw 159 more unique seeds before it creates the end and the nether
        // (measured: first sound seed of each level, end 177th and nether 180th draw).
        let void_world = !self.opts.flat && !self.opts.normal;
        let draws_before_level = match dimension {
            "overworld" => 2,
            "the_end" if void_world => 174,
            "the_end" => 15,
            "the_nether" if void_world || self.dims.contains(&"the_end") => 177,
            _ => 18,
        };
        for _ in 0..draws_before_level {
            mc_core::random::generate_unique_seed();
        }
        // one world data for the level and the command server (`/defaultgamemode` reaches `Projectile.canBreakBlockInAdventureMode`)
        let world_data = if mc_game::commands::server_access::has_server() {
            mc_game::commands::server_access::with_server(|server| server.world_data.clone())
        } else {
            mc_game::server::level::level_data::default_world_data()
        };
        world_data.lock().settings.difficulty_settings.difficulty = if self.opts.hardcore {
            Difficulty::Hard
        } else {
            self.opts.difficulty
        };
        world_data.lock().settings.difficulty_settings.hardcore = self.opts.hardcore;
        // `flat` and `void` are the oracle's flat generator, which the login and respawn packets report as a flat world
        world_data.lock().special_world_property = if self.opts.normal {
            mc_world::level_storage::SpecialWorldProperty::None
        } else {
            mc_world::level_storage::SpecialWorldProperty::Flat
        };
        if self.opts.flat {
            // the vanilla flat world's spawn (natural spawning keeps its distance from it)
            world_data.lock().respawn_data.pos = BlockPos::new(0, -60, 0);
        }
        self.world_data = Some(world_data.clone());
        let mut level = ServerLevel::new(ServerLevelParams {
            shared,
            dimension: Identifier::parse(&format!("minecraft:{dimension}")).unwrap(),
            level_id: "block_scenario".to_string(),
            dimension_dir: scratch.path().to_owned(),
            level_height: LevelHeight {
                min_y: dimension_type.min_y,
                height: dimension_type.height,
            },
            // the generator settings' sea levels (the replay has no noise settings; the flat generator's is -63)
            sea_level: match dimension {
                "overworld" if self.opts.normal => 63,
                "overworld" => -63,
                // `end void` / `nether void`: the flat generator's sea level
                _ if self.opts.void_dims.contains(&dimension) => -63,
                "the_nether" => 32,
                _ => 0,
            },
            has_sky_light: dimension_type.has_sky_light,
            factory: Arc::new(PalettedContainerFactory::create(biomes).expect("plains registered")),
            generator,
            generate_structures: self.opts.structures && noise_world.is_some(),
            hooks: match &noise_world {
                Some((env, _)) => Arc::new(
                    mc_game::world::level::levelgen::hooks::VanillaWorldGenHooks::new(env.clone())
                        .with_mob_spawner(Arc::new(
                            mc_game::world::level::spawner::chunk_gen::NaturalOriginalMobs,
                        )),
                ),
                None => Arc::new(mc_game::server::level::world_gen::NoWorldGenHooks),
            },
            seed: self.opts.seed,
            is_known_structure: if noise_world.is_some() {
                let structures = super::feature_harness::WORLD.structures.clone();
                Arc::new(move |id| {
                    Identifier::try_parse(id).is_some_and(|id| structures.get(&id).is_some())
                })
            } else {
                Arc::new(|_| false)
            },
            view_distance: 4,
            simulation_distance: 4,
            sync_writes: false,
            is_debug: false,
            level_data: Box::new(mc_game::server::level::level_data::PrimaryServerLevelData(
                world_data,
            )),
        });
        level.scratch_dir = Some(scratch);
        // dimension type attributes plus its timelines (the day timeline drives `SKY_LIGHT_LEVEL` and `SUN_ANGLE`), the weather layers and the biome layers
        level.tick_state.attributes = environment_attributes(
            &dimension_type,
            worldgen_biomes.as_deref(),
            self.clocks.clone(),
        );
        // the monster light test and block light limit of the dimension (the nether's limit is 15)
        level
            .spawning
            .install_dimension_type(Arc::new(dimension_type.clone()));
        // the coordinate scale and logical height portals read
        level.dimension_type = Some(Arc::new(dimension_type.clone()));
        // one entity id sequence for every level of the replay (the oracle's `ENTITY_COUNTER` is global; parallel tests must not share ids)
        match &self.entity_counter {
            Some(counter) => {
                level
                    .spawning
                    .gen_live
                    .share_entity_counter(counter.clone());
                level.entity_id_counter = Some(counter.clone());
            }
            None => {
                level.use_private_entity_counter(0);
                self.entity_counter = level.entity_id_counter.clone();
            }
        }
        // only the overworld ticks the time (`createLevels`: `tickTime`), the first level of a replay without one
        level.tick_state.tick_time = self.dims.first().is_none_or(|first| *first == dimension);
        if let Some(biomes) = worldgen_biomes {
            level.tick_state.biomes = Some(biomes);
            level.tick_state.biome_zoom_seed =
                mc_worldgen::biome::BiomeManager::obfuscate_seed(if noise_world.is_some() {
                    self.opts.seed
                } else {
                    4242
                });
            // the generation environment bone meal reads (feature lists, biome tags); the grown features never consult the generator
            level.gen_env = Some(super::feature_harness::gen_env(4242));
        } else {
            // `ClientboundRespawn` carries it as the hashed seed
            level.tick_state.biome_zoom_seed =
                mc_worldgen::biome::BiomeManager::obfuscate_seed(self.opts.seed);
        }
        if let Some((env, _)) = &noise_world {
            level.gen_env = Some(env.clone());
        } else if level.gen_env.is_none() && self.gen_env {
            level.gen_env = Some(super::feature_harness::gen_env(4242));
        }
        if self.flat_structures && self.opts.flat && dimension == "overworld" {
            level.gen_env = Some(super::feature_harness::flat_gen_env(self.opts.seed));
        }
        // the void levels of a multi-level server generate like the oracle's: the flat generator's plains, no structures (`/locate`)
        if !self.dims.is_empty() && noise_world.is_none() {
            level.gen_env = Some(super::feature_harness::flat_gen_env_in(
                dimension,
                self.opts.seed,
            ));
        }
        // `/place feature` with an inline definition decodes it against the worldgen registries of the level
        if level.gen_env.is_some() {
            level.worldgen_data = Some(super::feature_harness::WORLD.worldgen.clone());
        }
        level.has_ceiling = dimension_type.has_ceiling;
        // `Level.canHaveWeather()`
        level.tick_state.can_have_weather =
            dimension_type.has_sky_light && !dimension_type.has_ceiling && dimension != "the_end";
        level.tick_state.enabled_features = self.opts.features;
        level.tick_state.infiniburn = dimension_type.infiniburn.clone();
        // with a command environment the level reads the installed server's rules, scoreboard and command storage (`/gamerule`,
        // `/team`, `/data` edit the ones the level reads); else the replay keeps its own rules
        if mc_game::commands::server_access::has_server() {
            mc_game::commands::server_access::with_server(|server| {
                for (name, value) in &self.opts.boot_rules {
                    set_rule(server.game_rules(), name, value);
                }
                super::command_verbs::attach_server_state(&mut level, server);
            });
        } else {
            for (name, value) in &self.opts.boot_rules {
                set_rule(&mut self.env.game_rules, name, value);
            }
            level.server_state.rules.refresh(&self.env.game_rules);
        }
        apply_spawn_settings(&mut level);
        // `MinecraftServer.createLevels`: only the overworld has custom spawners (phantom, patrol, cat, siege, trader)
        if dimension == "overworld" {
            level.install_custom_spawners();
        }
        // `ServerLevel`'s constructor creates the fight of a dimension type that has one; the oracle drops it unless the scenario says `end fight`
        if self.opts.end_fight && dimension_type.has_ender_dragon_fight {
            fight::install(&mut level, self.opts.seed);
        }
        // chunk generation reads the rules from the values published per tick; the preload generates before the first one
        level.publish_spawn_environment(&self.env);
        // the border applies its saved settings when the level first reads it (the server does so while it creates the levels, before
        // any player listens): the change events of that first read are not player-visible
        level.get_world_border().take_events();
        // loot random sequences are seeded from the world seed (`VanillaLevel.Options.seed`)
        level.server_state.loot.world_seed = self.opts_seed();
        level.tick_state.broadcast_log = Some(self.log.clone());
        self.env.difficulty = self.opts.difficulty;
        if noise_world.is_some() {
            // the preload generates mobs and structure entities: they read the registries and the dimension type's attributes
            level.registries = actors::real_registries();
            level
                .spawning
                .gen_live
                .install(level.registries.clone(), Arc::new(dimension_type.clone()));
        }
        // `MinecraftServer.setInitialSpawn` searches the overworld's spawn once, when the server creates its levels: before any
        // preload ticket, so its chunks are the first the generator sees (`world_spawn`)
        if self.opts.find_world_spawn && dimension == "overworld" {
            let data = self.world_data.as_ref().expect("world data");
            mc_game::server::spawn::find_initial_spawn(data, &mut level);
        }
        let r = self.opts.preload;
        for cx in -r..=r {
            for cz in -r..=r {
                level
                    .chunk_source_mut()
                    .update_chunk_forced(ChunkPos::new(cx, cz), true);
            }
        }
        let start = Instant::now();
        if self.dims.last() == Some(&dimension) && !self.opts.settle_preload_ring {
            // `VanillaLevel.preload` of the server's last level: nothing polls the chunk source after it, so the work beyond what the
            // wait for the preload's entities and the chunk requests need stays queued until a later wait or tick
            let idle = || std::thread::sleep(Duration::from_micros(200));
            loop {
                level.process_pending_entity_loads();
                if (-r..=r).all(|cx| {
                    (-r..=r).all(|cz| level.are_entities_loaded(ChunkPos::pack_xz(cx, cz)))
                }) {
                    break;
                }
                assert!(
                    start.elapsed() < Duration::from_secs(120),
                    "the preload's entities did not load"
                );
                if !level.poll_chunk_task() {
                    idle();
                }
            }
            for cx in -r..=r {
                for cz in -r..=r {
                    let future = level.chunk_source_mut().get_chunk_future_main_thread(
                        cx,
                        cz,
                        mc_world::status::ChunkStatus::Full,
                        true,
                    );
                    while level.chunk_source().future_result(future).is_none() {
                        assert!(
                            start.elapsed() < Duration::from_secs(120),
                            "the preload's chunks did not load"
                        );
                        if !level.poll_chunk_task() {
                            idle();
                        }
                    }
                }
            }
        } else {
            self.preload_until_ticking(&mut level, r, start);
        }
        // the multi-level replays search after the preload: their recorded chunk counts depend on the chunks the search loads
        if !self.opts.find_world_spawn && self.multi() && dimension == "overworld" {
            let data = self.world_data.as_ref().expect("world data");
            mc_game::server::spawn::find_initial_spawn(data, &mut level);
        }
        use mc_core::random::RandomSource;
        level
            .tick_state
            .random
            .set_seed(self.opts_seed() ^ 0x5DEECE66D);
        // the oracle's references keep a removed entity: it dumps with `alive=0`
        level.removed_entities = Some(Default::default());
        level
    }

    /// The preload's wait of a level that is not the last one: every chunk step runs until the forced chunks are entity ticking (the
    /// later levels' waits keep polling this level's chunk source in the oracle until it has no work left).
    fn preload_until_ticking(&self, level: &mut ServerLevel, r: i32, start: Instant) {
        loop {
            level.tick_chunk_source(&|| true, false, true);
            while level.poll_chunk_task() {}
            level.tick_entity_management();
            let ready = (-r..=r).all(|cx| {
                (-r..=r).all(|cz| {
                    level.is_position_ticking_with_entities_loaded(ChunkPos::pack_xz(cx, cz))
                })
            });
            let ring = r + 2;
            let settled = !self.opts.settle_preload_ring
                || (-ring..=ring).all(|cx| {
                    (-ring..=ring).all(|cz| level.chunk_source().get_chunk_now(cx, cz).is_some())
                });
            if ready && settled {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(120),
                "chunks did not reach entity ticking"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
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
        // `in DIM LINE`: LINE runs with DIM as the level world verbs act on; its entities and seeds are those of LINE
        if a[0] == "in" {
            self.world();
            let dimension = self
                .dims
                .iter()
                .copied()
                .find(|d| *d == a[1])
                .unwrap_or_else(|| {
                    panic!("`in {}`: no such level in this replay (`levels`)", a[1])
                });
            let (outer_line, outer) = (self.line.clone(), self.current);
            let rest = outer_line.find(a[1].as_str()).expect("dimension") + a[1].len();
            self.line = outer_line[rest..].trim().to_owned();
            self.current = dimension;
            self.run(&a[2..]);
            (self.line, self.current) = (outer_line, outer);
            return;
        }
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
        // a line that names an entity (`give P ..`, `cmd_as P ..`) runs in that entity's level, any other in the current one
        if self.multi() {
            let named = a.get(1);
            match named.and_then(|n| self.actors.refs.iter().find(|(r, _)| r == n).map(|r| r.1)) {
                Some(id) => {
                    let dimension = self.dimension_of(id).unwrap_or(self.current);
                    self.focus(dimension);
                }
                None => self.focus(self.current),
            }
        }
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
                self.opts.normal = a[1] == "normal";
                self.opts.flat = match a[1].as_str() {
                    "flat" => true,
                    "void" | "normal" => false,
                    other => {
                        panic!(
                            "level type {other} is not supported by the Rust replay (flat|void|normal)"
                        )
                    }
                };
            }
            "structures" => {
                assert!(
                    self.level.is_none(),
                    "`structures` only before the first world command"
                );
                self.opts.structures = a[1].parse().expect("structures true|false");
            }
            "world_spawn" => {
                assert!(
                    self.level.is_none(),
                    "`world_spawn` only before the first world command"
                );
                self.opts.find_world_spawn = true;
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
            "seed" => {
                self.opts.seed = a[1].parse().expect("seed N");
                super::command_verbs::set_world_seed(self.opts.seed);
            }
            "difficulty" => {
                self.opts.difficulty = match a[1].as_str() {
                    "peaceful" => Difficulty::Peaceful,
                    "easy" => Difficulty::Easy,
                    "normal" => Difficulty::Normal,
                    "hard" => Difficulty::Hard,
                    other => panic!("difficulty {other}"),
                };
                // `/difficulty` of a running world
                if self.world_data.is_some() {
                    let command = format!("cmd difficulty {}", a[1]);
                    super::command_verbs::command(self, &tokens(&command), &command);
                }
            }
            "levels" => {
                assert!(
                    self.level.is_none(),
                    "`levels` only before the first world command"
                );
                self.opts.levels = a[1]
                    .split(',')
                    .map(|d| match d {
                        "overworld" => "overworld",
                        "the_nether" => "the_nether",
                        "the_end" => "the_end",
                        other => panic!("level {other}"),
                    })
                    .collect();
            }
            "end" | "nether" => {
                assert!(
                    self.level.is_none(),
                    "`end void`, `nether void` and `end fight` only before the first world command"
                );
                match (a[0].as_str(), a[1].as_str()) {
                    ("end", "fight") => self.opts.end_fight = true,
                    ("end", "void") => self.opts.void_dims.push("the_end"),
                    ("nether", "void") => self.opts.void_dims.push("the_nether"),
                    (dimension, other) => {
                        panic!("`{dimension} {other}`: end void|fight, nether void")
                    }
                }
            }
            "preload" => self.opts.preload = int(&a[1]),
            "preload_settle" => self.opts.settle_preload_ring = true,
            "hardcore" => self.opts.hardcore = true,
            "bootrule" => self.opts.boot_rules.push((a[1].clone(), a[2].clone())),
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
                self.env.game_rules = quiet_game_rules(self.opts.features);
            }
            "cmd" | "cmd_quiet" => {
                self.world();
                let line = self.line.clone();
                super::command_verbs::command(self, a, &line);
            }
            "note" => self.out.push(format!("# {}", self.line[4..].trim())),
            "gamerule" => {
                let command = format!("cmd gamerule {} {}", a[1], a[2]);
                super::command_verbs::command(self, &tokens(&command), &command);
            }
            "time" => {
                // the installed server's own clock, which the level reads
                let command = format!("cmd time set {}", a[1]);
                super::command_verbs::command(self, &tokens(&command), &command);
                self.for_each_level(|l| l.tick_state.attributes.invalidate_tick_cache());
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
            // `/fillbiome X1 Y1 Z1 X2 Y2 Z2 BIOME`: needs the real biome registry (see `run_scenario_text`)
            "biome" => {
                let command = format!("cmd fillbiome {}", a[1..8].join(" "));
                super::command_verbs::command(self, &tokens(&command), &command);
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
            "command_env" | "datapack" => {}
            "be_nbt" => {
                self.world();
                let snbt = self.line[self.line.find(a[4].as_str()).expect("snbt")..].to_string();
                self.be_nbt(pos(a, 1), &snbt);
            }
            "tick" => {
                self.world();
                self.remember_ref_uuids();
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
            _ if self.fight_command(a) => {}
            _ if self.persist_command(a) => {}
            _ if self.actor_command(a) => {}
            other => {
                panic!("command `{other}` is not supported by tests/common/block_scenario.rs yet")
            }
        }
    }

    /// `VanillaLevel.drainChunkTasks`: the vanilla server loop runs the chunk source's queued tasks while it waits for the next tick, so
    /// every chunk step the worker threads have finished is applied when a tick starts. Wait for the workers and drain, so a tick
    /// never depends on thread timing (the chunks `tickChunks` shuffles decide the level random draws).
    fn settle_chunks(&mut self) {
        self.for_each_level(|level| {
            loop {
                while level.poll_chunk_task() {}
                let map = &level.chunk_source().chunk_map;
                if map.pending_task_count() == 0 && map.loading_count() == 0 && !map.worldgen_busy()
                {
                    return;
                }
                std::thread::sleep(Duration::from_micros(200));
            }
        });
    }

    /// One real server tick; with `watch`es set the `T n` header and the dumps follow.
    pub fn tick_once(&mut self) {
        // `VanillaLevel.tick`: every tick restarts the entity-seed sequence
        mc_core::random::reseed_uniquifier(
            8682522807148012i64 ^ ((self.ticks + 1).wrapping_mul(0x9E3779B97F4A7C15u64 as i64)),
        );
        self.settle_chunks();
        self.apply_player_inputs();
        // `tickChildren` runs the `#minecraft:load` / `#minecraft:tick` functions first, before the time synchronization and the levels
        let has_server = mc_game::commands::server_access::has_server();
        if has_server {
            self.focus(self.current);
            let mut level = self.level.take().expect("world");
            mc_game::commands::server_access::with_server(|server| {
                server.with_lent_level(&mut level, |server| server.tick_functions())
            });
            self.level = Some(level);
        }
        self.force_game_time_synchronization();
        let mut sprinting = false;
        if has_server {
            // `tickServer` / `tickChildren` around the level ticks: the tick rate manager's step counter, then the clocks while the game runs
            // `runServer`: a requested sprint runs its ticks back to back with no time left
            sprinting = mc_game::commands::server_access::with_server(|server| {
                let sprinting = !server.is_paused()
                    && server.tick_rate_manager.is_sprinting()
                    && server.check_should_sprint_this_tick();
                server.tick_rate_manager.tick();
                if server.tick_rate_manager.runs_normally() {
                    server.tick_clocks();
                }
                sprinting
            });
        }
        if has_server {
            // `tickChildren`: the respawn data adjusted to the respawn level's world border, before the levels tick
            self.focus(self.current);
            let mut level = self.level.take().expect("world");
            mc_game::commands::server_access::with_server(|server| {
                server.with_lent_level(&mut level, |server| server.update_effective_respawn_data())
            });
            self.level = Some(level);
        }
        // `tickChildren` ticks the levels in the server's order, one after the other
        let dimensions = if self.multi() {
            self.dims.clone()
        } else {
            vec![self.at]
        };
        for dimension in dimensions {
            self.focus(dimension);
            let mut level = self.level.take().expect("world");
            if has_server {
                // a rule a command block changed since the last tick reaches the chunk source (`updateMobSpawningFlags`)
                apply_spawn_settings(&mut level);
                if self.multi() {
                    // the server's own level tick environment (`tickChildren`): the levels of the server tick like in production
                    let dimension = level.dimension.clone();
                    mc_game::commands::server_access::with_server(|server| {
                        server
                            .with_lent_level(&mut level, |server| {
                                server.with_level(&dimension.clone(), |server, level| {
                                    let mut env = mc_game::server::level_tick_env::ServerTickEnv {
                                        server,
                                        dimension,
                                    };
                                    level.tick(&mut env, &mut || !sprinting)
                                })
                            })
                            .expect("the ticking level")
                    });
                } else {
                    level.tick(&mut ServerBackedEnv(&mut self.env), &mut || !sprinting);
                }
            } else {
                level.tick(&mut self.env, &mut || true);
            }
            self.level = Some(level);
        }
        if sprinting {
            mc_game::commands::server_access::with_server(|server| {
                server.tick_rate_manager.end_tick_work()
            });
        }
        self.focus(self.current);
        self.tick_players();
        // vanilla's scoreboard listeners broadcast as they change; the replay sends what the tick queued
        if mc_game::commands::server_access::has_server() {
            mc_game::commands::server_access::with_server(|server| server.flush_scoreboard());
        }
        if mc_game::commands::server_access::has_server() {
            let level = self.level.as_mut().expect("world");
            mc_game::commands::server_access::with_server(|server| {
                server.with_lent_level(level, |server| server.flush_world_border_events())
            });
        }
        // `MinecraftServer.tickServer`: the debug subscribers are refreshed after the players
        self.for_each_level(|level| level.tick_debug_subscribers());
        self.send_chunks();
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
    /// they need the data pack registries and the generation environment `world` sets with the real biomes.
    fn bonemeal(&mut self, p: BlockPos) -> bool {
        self.load_registries();
        let level = self.level();
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
            // a dump of an entity (`entity:REF`, `packets:PLAYER`) reads its own level, any other the current one
            if self.multi() {
                match arg.and_then(|a| a.split([':', '/']).next()) {
                    Some(n) if self.actors.refs.iter().any(|(r, _)| r == n) => self.focus_ref(n),
                    _ => self.focus(self.current),
                }
            }
            match kind {
                "blocks" => {
                    let c: Vec<i32> = arg
                        .expect("blocks:x1,y1,z1,x2,y2,z2")
                        .split(',')
                        .map(int)
                        .collect();
                    assert_eq!(c.len(), 6, "blocks:x1,y1,z1,x2,y2,z2");
                    if self.multi() {
                        // the oracle reads through `Level.getBlockState`, which asks the chunk source for the full chunk: that adds
                        // the one-tick `UNKNOWN` ticket that keeps the chunk (and its neighbours) from unloading in that tick
                        for cx in (c[0].min(c[3]) >> 4)..=(c[0].max(c[3]) >> 4) {
                            for cz in (c[2].min(c[5]) >> 4)..=(c[2].max(c[5]) >> 4) {
                                self.level()
                                    .chunk_source_mut()
                                    .get_chunk_future_main_thread(
                                        cx,
                                        cz,
                                        mc_world::status::ChunkStatus::Full,
                                        true,
                                    );
                            }
                        }
                    }
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
                "light" => {
                    let c: Vec<i32> = arg.expect("light:x,y,z").split(',').map(int).collect();
                    let line = self.light_line(BlockPos::new(c[0], c[1], c[2]));
                    self.out.push(line);
                }
                "lights" => {
                    let c: Vec<i32> = arg
                        .expect("lights:x1,y1,z1,x2,y2,z2")
                        .split(',')
                        .map(int)
                        .collect();
                    assert_eq!(c.len(), 6, "lights:x1,y1,z1,x2,y2,z2");
                    for x in c[0].min(c[3])..=c[0].max(c[3]) {
                        for y in c[1].min(c[4])..=c[1].max(c[4]) {
                            for z in c[2].min(c[5])..=c[2].max(c[5]) {
                                let line = self.light_line(BlockPos::new(x, y, z));
                                if !line.ends_with("block=0 sky=0") {
                                    self.out.push(line);
                                }
                            }
                        }
                    }
                }
                "ticks" => self.dump_ticks(arg),
                _ if self.dump_fight(kind) => {}
                _ if self.dump_persist(kind, arg) => {}
                _ if self.dump_actor(kind, arg) => {}
                _ if super::command_verbs::dump(self, kind, arg) => {}
                "events" => self.dump_events(),
                other => panic!("dump {other} is not supported by tests/common/block_scenario.rs"),
            }
        }
    }

    /// `LIGHT x y z block=B sky=S`: `getBrightness` of both light layers.
    fn light_line(&mut self, p: BlockPos) -> String {
        let level = self.level();
        let block = level.get_brightness(LightLayer::Block, p);
        let sky = level.get_brightness(LightLayer::Sky, p);
        format!(
            "LIGHT {} {} {} block={block} sky={sky}",
            p.x(),
            p.y(),
            p.z()
        )
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
/// `MinecraftServer.updateMobSpawningFlags`: the chunk source spawns monsters while `spawn_mobs` and `spawn_monsters` are on.
fn apply_spawn_settings(level: &mut ServerLevel) {
    let spawn = level.spawn_mobs() && level.spawn_monsters();
    level.chunk_source_mut().set_spawn_settings(spawn);
}

/// `VanillaLevel.boot`: the frozen, quiet world's rules (a rule that needs a feature flag exists only when the world enables it).
pub(super) fn quiet_game_rules(features: FeatureFlagSet) -> GameRules {
    let mut game_rules = GameRules::new(features);
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
        "log_admin_commands",
        "send_command_feedback",
    ] {
        set_rule(&mut game_rules, rule, "false");
    }
    set_rule(&mut game_rules, "random_tick_speed", "0");
    game_rules
}

pub(super) fn set_rule(rules: &mut GameRules, name: &str, value: &str) {
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
