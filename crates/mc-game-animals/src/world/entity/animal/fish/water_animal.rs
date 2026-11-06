//! `net.minecraft.world.entity.animal.fish.WaterAnimal` and `net.minecraft.world.entity.animal.AgeableWaterCreature`: the
//! shared water-bound behavior (air supply while out of water, no fluid push, no leash, 1-3 xp, spawn obstruction).
//!
//! Concrete classes call these from their `Entity` impl; [`water_creature_entity!`] writes the overrides both Java base
//! classes make.

use mc_core::random::RandomSource as _;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::living_entity::util::hurt;
use mc_game_core::world::entity::mob;
use mc_game_core::world::level::level_access::{BlockGetter as _, LevelAccessor as _};
use mc_game_core::world::level::material::FluidStateExt as _;

/// `AMBIENT_SOUND_INTERVAL`.
pub const AMBIENT_SOUND_INTERVAL: i32 = 120;

/// `WaterAnimal.handleAirSupply(level, preTickAirSupply)` / `AgeableWaterCreature.handleAirSupply(preTickAirSupply)`.
pub fn handle_air_supply(e: &mut dyn Entity, level: &mut ServerLevel, pre_tick_air_supply: i32) {
    if e.is_alive() && !e.is_in_water() {
        e.base_mut().set_air_supply(pre_tick_air_supply - 1);
        if e.should_take_drowning_damage() {
            e.base_mut().set_air_supply(0);
            hurt(level, e, |sources| sources.drown(), 2.0);
        }
    } else {
        e.base_mut().set_air_supply(300);
    }
}

/// `WaterAnimal.baseTick()` / `AgeableWaterCreature.baseTick()`.
pub fn base_tick(e: &mut dyn Entity, level: &mut ServerLevel) {
    let air_supply = e.base().get_air_supply();
    mob::tick::base_tick(e, level);
    handle_air_supply(e, level, air_supply);
}

/// `getBaseExperienceReward`: `1 + random.nextInt(3)`.
pub fn get_base_experience_reward(e: &mut dyn Entity) -> i32 {
    1 + e.base_mut().random.next_int_bounded(3)
}

/// The overrides `WaterAnimal` / `AgeableWaterCreature` make of `Mob`/`Entity`, for an `impl Entity` block.
macro_rules! water_creature_entity {
    () => {
        crate::world::entity::animal::fish::water_animal::water_creature_entity!(no_leash);
        fn can_be_leashed(&self) -> bool {
            false
        }
    };
    (no_leash) => {
        fn base_tick(&mut self, level: &mut mc_game_core::server::level::ServerLevel) {
            crate::world::entity::animal::fish::water_animal::base_tick(self, level);
        }
        fn is_pushed_by_fluid(&self) -> bool {
            false
        }
        fn check_spawn_obstruction(
            &self,
            level: &dyn mc_game_core::world::level::server_level_accessor::ServerLevelAccessor,
        ) -> bool {
            level.is_unobstructed_entity(self)
        }
        fn get_ambient_sound_interval(&self) -> i32 {
            crate::world::entity::animal::fish::water_animal::AMBIENT_SOUND_INTERVAL
        }
        fn get_base_experience_reward(
            &mut self,
            _level: &mc_game_core::server::level::ServerLevel,
        ) -> i32 {
            crate::world::entity::animal::fish::water_animal::get_base_experience_reward(self)
        }
    };
}
pub(crate) use water_creature_entity;
