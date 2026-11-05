//! Mutable per-level state of `Level`/`ServerLevel` that belongs to mutation and ticking: the neighbor updater, scheduled
//! block/fluid ticks, block events, block entity tickers, sub tick counter, random state, weather levels.

use super::ServerLevel;
use super::block_events::BlockEvents;
use crate::world::level::chunk::TickerCell;
use crate::world::level::redstone::CollectingNeighborUpdater;
use mc_core::random::{LegacyRandomSource, RandomSource, ThreadSafeLegacyRandomSource, create_new, create_thread_local_instance_new};
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_data::fluid::Fluid;
use mc_world::level_storage::FeatureFlagSet;
use mc_world::ticks::LevelTicks;
use parking_lot::Mutex;
use std::rc::Rc;
use std::sync::Arc;

/// `Level.random` as a shared handle: vanilla passes the very same `RandomSource` object to `Feature.place(level, ..,
/// random, ..)` that the level's own block updates draw from, so the call interleaving is observable. A clone of the
/// handle can be passed as the `random` argument while the level itself is borrowed mutably as the `WorldGenLevel`.
#[derive(Clone, Debug)]
pub struct LevelRandom(Arc<Mutex<LegacyRandomSource>>);

impl LevelRandom {
    pub fn new(random: LegacyRandomSource) -> Self {
        Self(Arc::new(Mutex::new(random)))
    }
}

impl RandomSource for LevelRandom {
    fn fork(&mut self) -> Box<dyn RandomSource> {
        self.0.lock().fork()
    }
    fn fork_positional(&mut self) -> Box<dyn mc_core::random::PositionalRandomFactory> {
        self.0.lock().fork_positional()
    }
    fn set_seed(&mut self, seed: i64) {
        self.0.lock().set_seed(seed);
    }
    fn next_int(&mut self) -> i32 {
        self.0.lock().next_int()
    }
    fn next_int_bounded(&mut self, bound: i32) -> i32 {
        self.0.lock().next_int_bounded(bound)
    }
    fn next_long(&mut self) -> i64 {
        self.0.lock().next_long()
    }
    fn next_boolean(&mut self) -> bool {
        self.0.lock().next_boolean()
    }
    fn next_float(&mut self) -> f32 {
        self.0.lock().next_float()
    }
    fn next_double(&mut self) -> f64 {
        self.0.lock().next_double()
    }
    fn next_gaussian(&mut self) -> f64 {
        self.0.lock().next_gaussian()
    }
    fn legacy_next(&mut self, bits: i32) -> Option<i32> {
        self.0.lock().legacy_next(bits)
    }
}

/// `max-chained-neighbor-updates` default (server.properties).
pub const DEFAULT_MAX_CHAINED_NEIGHBOR_UPDATES: i32 = 1_000_000;

/// `MAX_SCHEDULED_TICKS_PER_TICK`.
pub const MAX_SCHEDULED_TICKS_PER_TICK: usize = 65536;

/// `ServerLevel.EMPTY_TIME_NO_TICK`.
pub const EMPTY_TIME_NO_TICK: i32 = 300;

/// The scheduled tick containers of every loaded chunk. Shared (`Arc<Mutex<..>>`) because the chunk map must read the
/// pending ticks of a chunk when it saves it (vanilla shares the `LevelChunkTicks` objects with the chunk).
#[derive(Default)]
pub struct TickContainers {
    pub block: LevelTicks<Block>,
    pub fluid: LevelTicks<Fluid>,
}

pub type SharedTickContainers = Arc<Mutex<TickContainers>>;

/// A block entity that is out of its chunk while its code runs (`with_block_entity`): `setBlock` calls on its position
/// from inside that code must neither create a second block entity nor lose this one.
pub struct TakenBlockEntity {
    pub pos: BlockPos,
    /// The state `setBlock` put at `pos` while the block entity was out (same block): applied on put-back.
    pub new_state: Option<mc_data::block::BlockState>,
    /// `setBlock` replaced the block (or removed the block entity): it must not be put back.
    pub removed: bool,
}

pub struct LevelTickState {
    /// `neighborUpdater`.
    pub neighbor_updater: CollectingNeighborUpdater,
    /// `blockTicks` / `fluidTicks`.
    pub ticks: SharedTickContainers,
    /// `blockEvents` (+ `blockEventsToReschedule`).
    pub block_events: BlockEvents,
    /// `blockEntityTickers` / `pendingBlockEntityTickers` / `tickingBlockEntities`.
    pub block_entity_tickers: Vec<Rc<TickerCell>>,
    pub pending_block_entity_tickers: Vec<Rc<TickerCell>>,
    pub ticking_block_entities: bool,
    /// The tick environment while block ticks run (see `level_commands.rs`).
    pub command_env: Option<super::level_commands::CommandEnvPtr>,
    /// The command block game rules at the start of the last tick (`ServerLevel::command_rules` outside block ticks).
    pub command_rules: super::level_commands::CommandRules,
    /// `subTickCount`.
    pub sub_tick_count: i64,
    /// `randValue` (`getBlockRandomPos`).
    pub rand_value: i32,
    /// `Level.random`.
    pub random: LevelRandom,
    /// `soundSeedGenerator`.
    pub sound_seed_generator: ThreadSafeLegacyRandomSource,
    /// `handlingTick`.
    pub handling_tick: bool,
    /// `emptyTime`.
    pub empty_time: i32,
    /// `server.getWorldData().enabledFeatures()` (set by the world loader).
    pub enabled_features: FeatureFlagSet,
    /// Called when a block state change swaps the point of interest type at a position (`updatePOIOnBlockStateChange`);
    /// arguments are the old and new `mc_data::misc::POI_TYPES` index. Set by the POI manager.
    pub poi_hook: Option<fn(&mut ServerLevel, BlockPos, Option<usize>, Option<usize>)>,
    /// `canHaveWeather()` (`dimensionType.hasSkyLight && !hasCeiling && dimension != END`).
    pub can_have_weather: bool,
    /// `dimensionType().hasFixedTime()`.
    pub has_fixed_time: bool,
    /// `dimensionType().ambientLight()`.
    pub ambient_light: f32,
    /// `tickTime` constructor flag (only the overworld advances the game time).
    pub tick_time: bool,
    /// `sleepStatus`.
    pub sleep_status: super::sleep_status::SleepStatus,
    /// The biome registry and `BiomeManager` zoom seed (`BiomeManager.obfuscateSeed(seed)`), set by the world loader.
    pub biomes: Option<Arc<mc_worldgen::biome::BiomeRegistry>>,
    pub biome_zoom_seed: i64,
    /// `tickThunder` spawn of the lightning bolt (and the skeleton horse of a trap): `(level, pos, is_trap)`.
    pub lightning_spawn: Option<fn(&mut ServerLevel, BlockPos, bool)>,
    /// `findLightningRod(center)` (point of interest lookup): the rod's block position.
    pub find_lightning_rod: Option<fn(&mut ServerLevel, BlockPos) -> Option<BlockPos>>,
    /// `dragonFight.tick()`.
    pub dragon_fight_tick: Option<fn(&mut ServerLevel)>,
    /// `environmentAttributes`: dimension constants + timelines + weather layers (set at level creation).
    pub attributes: crate::world::attribute::EnvironmentAttributeSystem,
    /// `oRainLevel` / `rainLevel` / `oThunderLevel` / `thunderLevel`.
    pub o_rain_level: f32,
    pub rain_level: f32,
    pub o_thunder_level: f32,
    pub thunder_level: f32,
    /// Entities whose `PortalProcessor` decided to teleport this tick (see `world::entity::portal`).
    pub pending_portal_teleports: Vec<crate::world::entity::EntityId>,
    /// Block entities currently taken out of their chunk by `ServerLevel::with_block_entity` (innermost last).
    pub taken_block_entities: Vec<TakenBlockEntity>,
    /// `RedstoneTorchBlock.RECENT_TOGGLES` of this level: `(pos, game time)` in insertion order.
    pub recent_redstone_torch_toggles: Vec<(BlockPos, i64)>,
}

impl LevelTickState {
    pub fn new(max_chained_neighbor_updates: i32) -> Self {
        Self {
            neighbor_updater: CollectingNeighborUpdater::new(max_chained_neighbor_updates),
            ticks: Arc::new(Mutex::new(TickContainers::default())),
            block_events: BlockEvents::default(),
            block_entity_tickers: Vec::new(),
            pending_block_entity_tickers: Vec::new(),
            ticking_block_entities: false,
            command_env: None,
            command_rules: Default::default(),
            sub_tick_count: 0,
            rand_value: create_thread_local_instance_new().next_int(),
            random: LevelRandom::new(create_new()),
            sound_seed_generator: mc_core::random::create_thread_safe(),
            enabled_features: FeatureFlagSet::DEFAULT_FLAGS,
            poi_hook: None,
            can_have_weather: false,
            has_fixed_time: false,
            ambient_light: 0.0,
            tick_time: false,
            sleep_status: super::sleep_status::SleepStatus::default(),
            biomes: None,
            biome_zoom_seed: 0,
            lightning_spawn: Some(crate::world::entity::lightning_bolt::spawn_from_thunder),
            find_lightning_rod: None,
            dragon_fight_tick: None,
            attributes: crate::world::attribute::EnvironmentAttributeSystem::build(&Default::default(), &Vec::new(), &[], false, Default::default()),
            handling_tick: false,
            empty_time: 0,
            o_rain_level: 0.0,
            rain_level: 0.0,
            o_thunder_level: 0.0,
            thunder_level: 0.0,
            pending_portal_teleports: Vec::new(),
            taken_block_entities: Vec::new(),
            recent_redstone_torch_toggles: Vec::new(),
        }
    }
}

impl Default for LevelTickState {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_CHAINED_NEIGHBOR_UPDATES)
    }
}
