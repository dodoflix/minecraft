//! The shared spawn predicates of vanilla's spawn placement table (`SpawnPlacements.SpawnPredicate` statics on `Mob`,
//! `Animal`, `Monster`, `WaterAnimal` and `AgeableWaterCreature`), all with the [`SpawnPredicate`](super::spawn_placements::SpawnPredicate)
//! signature so mob classes can call them from their own predicates (`Monster.checkMonsterSpawnRules(type, level, ..)`).

use crate::world::level::block::BlockStateExt as _;
use crate::world::level::material::FluidStateExt as _;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use crate::world::level::{BlockGetter as _, LevelReader as _, LightLayer};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::EntityType;
use mc_data::block::Block;
use mc_data::enums::EntitySpawnReason;

/// `EntitySpawnReason.isSpawner(reason)`.
pub fn is_spawner(reason: EntitySpawnReason) -> bool {
    reason == EntitySpawnReason::Spawner || reason == EntitySpawnReason::TrialSpawner
}

/// `EntitySpawnReason.ignoresLightRequirements(reason)`.
pub fn ignores_light_requirements(reason: EntitySpawnReason) -> bool {
    reason == EntitySpawnReason::TrialSpawner
}

/// `Mob.checkMobSpawnRules`.
pub fn check_mob_spawn_rules(
    ty: EntityType,
    level: &dyn ServerLevelAccessor,
    reason: EntitySpawnReason,
    pos: BlockPos,
    _random: &mut dyn RandomSource,
) -> bool {
    let below = pos.below();
    is_spawner(reason)
        || level
            .get_block_state(below)
            .is_valid_spawn(level, below, ty)
}

/// `Animal.isBrightEnoughToSpawn(level, pos)`.
pub fn is_bright_enough_to_spawn(level: &dyn ServerLevelAccessor, pos: BlockPos) -> bool {
    level.get_raw_brightness(pos, 0) > 8
}

/// `Animal.checkAnimalSpawnRules`.
pub fn check_animal_spawn_rules(
    _ty: EntityType,
    level: &dyn ServerLevelAccessor,
    reason: EntitySpawnReason,
    pos: BlockPos,
    _random: &mut dyn RandomSource,
) -> bool {
    let bright_enough_to_spawn =
        ignores_light_requirements(reason) || is_bright_enough_to_spawn(level, pos);
    level
        .get_block_state(pos.below())
        .is_tag("animals_spawnable_on")
        && bright_enough_to_spawn
}

/// `Monster.isDarkEnoughToSpawn(level, pos, random)`.
pub fn is_dark_enough_to_spawn(
    level: &dyn ServerLevelAccessor,
    pos: BlockPos,
    random: &mut dyn RandomSource,
) -> bool {
    if level.get_brightness(LightLayer::Sky, pos) > random.next_int_bounded(32) {
        return false;
    }
    let block_light_limit = level.monster_spawn_block_light_limit();
    if block_light_limit < 15 && level.get_brightness(LightLayer::Block, pos) > block_light_limit {
        return false;
    }
    let brightness = if level.is_thundering() {
        level.get_max_local_raw_brightness_with(pos, 10)
    } else {
        level.get_max_local_raw_brightness(pos)
    };
    brightness <= level.monster_spawn_light_test(random)
}

/// `Monster.checkMonsterSpawnRules`.
pub fn check_monster_spawn_rules(
    ty: EntityType,
    level: &dyn ServerLevelAccessor,
    reason: EntitySpawnReason,
    pos: BlockPos,
    random: &mut dyn RandomSource,
) -> bool {
    (ignores_light_requirements(reason) || is_dark_enough_to_spawn(level, pos, random))
        && check_mob_spawn_rules(ty, level, reason, pos, random)
}

/// `Monster.checkAnyLightMonsterSpawnRules`.
pub fn check_any_light_monster_spawn_rules(
    ty: EntityType,
    level: &dyn ServerLevelAccessor,
    reason: EntitySpawnReason,
    pos: BlockPos,
    random: &mut dyn RandomSource,
) -> bool {
    check_mob_spawn_rules(ty, level, reason, pos, random)
}

/// `Monster.checkSurfaceMonstersSpawnRules`.
pub fn check_surface_monsters_spawn_rules(
    ty: EntityType,
    level: &dyn ServerLevelAccessor,
    reason: EntitySpawnReason,
    pos: BlockPos,
    random: &mut dyn RandomSource,
) -> bool {
    check_monster_spawn_rules(ty, level, reason, pos, random)
        && (is_spawner(reason) || level.can_see_sky(pos))
}

/// `WaterAnimal.checkSurfaceWaterAnimalSpawnRules`.
pub fn check_surface_water_animal_spawn_rules(
    _ty: EntityType,
    level: &dyn ServerLevelAccessor,
    _reason: EntitySpawnReason,
    pos: BlockPos,
    _random: &mut dyn RandomSource,
) -> bool {
    let sea_level = level.get_sea_level();
    let min_spawn_level = sea_level - 13;
    pos.y() >= min_spawn_level
        && pos.y() <= sea_level
        && level.get_fluid_state(pos.below()).is_tag("water")
        && level.get_block_state(pos.above()).block() == Block::WATER
}

/// `AgeableWaterCreature.checkSurfaceAgeableWaterCreatureSpawnRules`.
pub fn check_surface_ageable_water_creature_spawn_rules(
    ty: EntityType,
    level: &dyn ServerLevelAccessor,
    reason: EntitySpawnReason,
    pos: BlockPos,
    random: &mut dyn RandomSource,
) -> bool {
    check_surface_water_animal_spawn_rules(ty, level, reason, pos, random)
}
