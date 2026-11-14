//! `WanderingTraderSpawner`: the overworld's periodic wandering trader (and two leashed trader llamas) spawn.

use super::wandering_trader::WanderingTrader;
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::{LegacyRandomSource, RandomSource, create, create_new};
use mc_data::EntityType;
use mc_data::enums::EntitySpawnReason;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::EntityId;
use mc_game_core::world::entity::ai::village::poi::Occupancy;
use mc_game_core::world::entity::mob::leashable::{self, LeashHolder};
use mc_game_core::world::entity::spawn_config;
use mc_game_core::world::entity::spawn_placements::{self, SpawnPlacementTypeExt};
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::saveddata::wandering_trader_data::WanderingTraderData;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_game_core::world::level::spawner::custom_spawner::{
    CUSTOM_SPAWNERS, CustomSpawner, CustomSpawnerRegistration, delay_option, order,
};
use mc_game_core::world::level::{BlockGetter, LevelReader};
use std::collections::BTreeMap;

const DEFAULT_TICK_DELAY: i32 = 1200;
pub const DEFAULT_SPAWN_DELAY: i32 = 24000;
pub const MIN_SPAWN_CHANCE: i32 = 25;
const MAX_SPAWN_CHANCE: i32 = 75;
const SPAWN_CHANCE_INCREASE: i32 = 25;
const SPAWN_ONE_IN_X_CHANCE: i32 = 10;
const NUMBER_OF_SPAWN_ATTEMPTS: i32 = 10;

/// `WanderingTraderSpawner`.
pub struct WanderingTraderSpawner {
    /// `RandomSource.create()`: its own, unseeded random.
    random: LegacyRandomSource,
    tick_delay: i32,
}

impl WanderingTraderSpawner {
    pub fn new() -> Self {
        Self {
            random: create_new(),
            tick_delay: DEFAULT_TICK_DELAY,
        }
    }

    /// `getTraderData()` (`savedDataStorage.computeIfAbsent(WanderingTraderData.TYPE)`).
    fn trader_data(level: &mut ServerLevel) -> &mut WanderingTraderData {
        level
            .get_data_storage()
            .compute_if_absent(&WanderingTraderData::type_())
    }

    /// `spawn(level)`.
    fn spawn(&mut self, level: &mut ServerLevel) -> bool {
        let Some(player) = level.get_random_player() else {
            return true;
        };
        if self.random.next_int_bounded(SPAWN_ONE_IN_X_CHANCE) != 0 {
            return false;
        }
        let Some(player_pos) = level.get_entity(player).map(|p| p.block_position()) else {
            return true;
        };
        let radius = 48;
        let poi_pos = level.poi_manager().find(
            &|t| t.registered_name() == "minecraft:meeting",
            &|_| true,
            player_pos,
            radius,
            Occupancy::Any,
        );
        let reference_pos = poi_pos.unwrap_or(player_pos);
        let Some(spawn_position) = self
            .find_spawn_position_near(level, reference_pos, radius)
            .filter(|p| has_enough_space(level, *p))
        else {
            return false;
        };
        if level.biome_is_tag(spawn_position, "without_wandering_trader_spawns") {
            return false;
        }
        let Some(trader) = spawn_config::spawn_at(
            level,
            EntityType::WANDERING_TRADER,
            None,
            spawn_position,
            EntitySpawnReason::Event,
            false,
            false,
        ) else {
            return false;
        };
        for _ in 0..2 {
            self.try_to_spawn_llama_for(level, trader, 4);
        }
        level.with_entity(trader, |_, e| {
            if let Some(t) = e.as_any_mut().downcast_mut::<WanderingTrader>() {
                t.set_despawn_delay(48000);
                t.set_wander_target(Some(reference_pos));
                t.mob.set_home_to(reference_pos, 16);
            }
        });
        true
    }

    /// `tryToSpawnLlamaFor(level, trader, radius)`.
    fn try_to_spawn_llama_for(&mut self, level: &mut ServerLevel, trader: EntityId, radius: i32) {
        let Some(trader_pos) = level.get_entity(trader).map(|t| t.block_position()) else {
            return;
        };
        let Some(spawn_position) = self.find_spawn_position_near(level, trader_pos, radius) else {
            return;
        };
        let Some(llama) = spawn_config::spawn_at(
            level,
            EntityType::TRADER_LLAMA,
            None,
            spawn_position,
            EntitySpawnReason::Event,
            false,
            false,
        ) else {
            return;
        };
        let Some(holder) = level.get_entity(trader).map(LeashHolder::of) else {
            return;
        };
        level.with_entity(llama, |level, e| {
            leashable::set_leashed_to(e, level, holder, true);
        });
    }

    /// `findSpawnPositionNear(level, referencePosition, radius)`.
    fn find_spawn_position_near(
        &mut self,
        level: &ServerLevel,
        reference_position: BlockPos,
        radius: i32,
    ) -> Option<BlockPos> {
        let placement = spawn_placements::get_placement_type(EntityType::WANDERING_TRADER);
        let heightmap = spawn_placements::get_heightmap_type(EntityType::WANDERING_TRADER);
        for _ in 0..NUMBER_OF_SPAWN_ATTEMPTS {
            let x = reference_position.x() + self.random.next_int_bounded(radius * 2) - radius;
            let z = reference_position.z() + self.random.next_int_bounded(radius * 2) - radius;
            let y = level.get_height_at(heightmap, x, z);
            let spawn_pos = BlockPos::new(x, y, z);
            if placement.is_spawn_position_ok(level, spawn_pos, Some(EntityType::WANDERING_TRADER))
            {
                return Some(spawn_pos);
            }
        }
        None
    }
}

impl Default for WanderingTraderSpawner {
    fn default() -> Self {
        Self::new()
    }
}

/// `hasEnoughSpace(level, spawnPos)`: every block of the 2x3x2 box from `spawnPos` has an empty collision shape.
fn has_enough_space(level: &ServerLevel, spawn_pos: BlockPos) -> bool {
    (spawn_pos.x()..=spawn_pos.x() + 1).all(|x| {
        (spawn_pos.y()..=spawn_pos.y() + 2).all(|y| {
            (spawn_pos.z()..=spawn_pos.z() + 1).all(|z| {
                let pos = BlockPos::new(x, y, z);
                level
                    .get_block_state(pos)
                    .get_collision_shape(level, pos)
                    .is_empty()
            })
        })
    })
}

impl CustomSpawner for WanderingTraderSpawner {
    fn tick(&mut self, level: &mut ServerLevel, _spawn_enemies: bool) -> i32 {
        if !level.spawn_wandering_traders() {
            return 0;
        }
        self.tick_delay -= 1;
        if self.tick_delay > 0 {
            return 0;
        }
        self.tick_delay = DEFAULT_TICK_DELAY;
        let data = Self::trader_data(level);
        let spawn_delay = data.spawn_delay() - DEFAULT_TICK_DELAY;
        data.set_spawn_delay(spawn_delay);
        if spawn_delay <= 0 {
            data.set_spawn_delay(DEFAULT_SPAWN_DELAY);
            let chance_to_spawn = data.spawn_chance();
            data.set_spawn_chance(mth::clamp_i32(
                chance_to_spawn + SPAWN_CHANCE_INCREASE,
                MIN_SPAWN_CHANCE,
                MAX_SPAWN_CHANCE,
            ));
            if self.random.next_int_bounded(100) <= chance_to_spawn && self.spawn(level) {
                Self::trader_data(level).set_spawn_chance(MIN_SPAWN_CHANCE);
            }
        }
        0
    }

    fn replay_setup(
        &mut self,
        level: &mut ServerLevel,
        seed: i64,
        options: &BTreeMap<String, String>,
    ) {
        self.random = create(seed);
        let number = |key: &str| options.get(key).map(|v| v.parse::<i32>().expect(key));
        if let Some(delay) = delay_option(options) {
            self.tick_delay = delay;
        }
        if let Some(spawn_delay) = number("spawn_delay") {
            Self::trader_data(level).set_spawn_delay(spawn_delay);
        }
        if let Some(chance) = number("chance") {
            Self::trader_data(level).set_spawn_chance(chance);
        }
    }
}

#[distributed_slice(CUSTOM_SPAWNERS)]
static WANDERING_TRADER_SPAWNER: CustomSpawnerRegistration = CustomSpawnerRegistration {
    order: order::WANDERING_TRADER,
    create: |_| Some(Box::new(WanderingTraderSpawner::new())),
};
