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
    /// `subTickCount`.
    pub sub_tick_count: i64,
    /// `randValue` (`getBlockRandomPos`).
    pub rand_value: i32,
    /// `Level.random`.
    pub random: LegacyRandomSource,
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
    /// `tickTime` constructor flag (only the overworld advances the game time).
    pub tick_time: bool,
    /// The game rules read by the random-tick code, refreshed at the start of every level tick.
    pub rules: super::level_tick::LevelRules,
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
            sub_tick_count: 0,
            rand_value: create_thread_local_instance_new().next_int(),
            random: create_new(),
            sound_seed_generator: mc_core::random::create_thread_safe(),
            enabled_features: FeatureFlagSet::DEFAULT_FLAGS,
            poi_hook: None,
            can_have_weather: false,
            tick_time: false,
            rules: super::level_rules::LevelRules::default(),
            sleep_status: super::sleep_status::SleepStatus::default(),
            biomes: None,
            biome_zoom_seed: 0,
            lightning_spawn: None,
            find_lightning_rod: None,
            dragon_fight_tick: None,
            attributes: crate::world::attribute::EnvironmentAttributeSystem::build(&Default::default(), &[], false),
            handling_tick: false,
            empty_time: 0,
            o_rain_level: 0.0,
            rain_level: 0.0,
            o_thunder_level: 0.0,
            thunder_level: 0.0,
            pending_portal_teleports: Vec::new(),
        }
    }
}

impl Default for LevelTickState {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_CHAINED_NEIGHBOR_UPDATES)
    }
}
