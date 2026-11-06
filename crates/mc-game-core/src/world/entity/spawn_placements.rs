//! `net.minecraft.world.entity.SpawnPlacements` + `SpawnPlacementTypes`: where each entity type may spawn naturally
//! (placement type + heightmap) and the per-type spawn predicate.
//!
//! The placement type and heightmap of every vanilla registration come from `mc_data` (`entity_types.json`
//! `spawn_placement`, i.e. vanilla's `SpawnPlacements.static {}`); the predicates are code:
//! * the shared predicates (`Mob/Animal/Monster/WaterAnimal/AgeableWaterCreature` statics, see
//!   [`spawn_predicates`](super::spawn_predicates)) are bound to their entity types by the table below (exactly vanilla's
//!   `register(type, ...)` lines that use them);
//! * mob-specific predicates (`Ocelot::checkOcelotSpawnRules`, `Guardian::checkGuardianSpawnRules`, ...) are registered by
//!   the mob's module with [`register_spawn_placement!`](crate::register_spawn_placement):
//!   ```ignore
//!   fn check_ocelot_spawn_rules(ty: EntityType, level: &dyn ServerLevelAccessor, reason: EntitySpawnReason, pos: BlockPos, random: &mut dyn RandomSource) -> bool { .. }
//!   register_spawn_placement!(EntityType::OCELOT, check_ocelot_spawn_rules);
//!   ```
//!   A type registered twice (here and in a mob module) panics at first use, like vanilla's "Duplicate registration".

use super::spawn_predicates as shared;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use crate::world::level::spawner::natural_spawner;
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::EntityType;
use mc_data::block::PathComputationType;
use mc_data::enums::{EntitySpawnReason, HeightmapTypes, SpawnPlacementType};
use mc_world::heightmap::HeightmapType;
use std::collections::HashMap;
use std::sync::LazyLock;

/// `SpawnPlacements.SpawnPredicate<T>`: `test(type, level, spawnReason, pos, random)`.
pub type SpawnPredicate = fn(
    EntityType,
    &dyn ServerLevelAccessor,
    EntitySpawnReason,
    BlockPos,
    &mut dyn RandomSource,
) -> bool;

/// One mob-specific `SpawnPlacements.register(type, placement, heightmap, predicate)` (placement and heightmap are the
/// `mc_data` ones of the type).
pub struct SpawnPlacementRegistration {
    pub entity_type: EntityType,
    pub predicate: SpawnPredicate,
}

#[distributed_slice]
pub static SPAWN_PLACEMENTS: [SpawnPlacementRegistration];

/// Registers the spawn predicate of an entity type from its module (see the module docs).
#[macro_export]
macro_rules! register_spawn_placement {
    ($entity_type:expr, $predicate:path) => {
        const _: () = {
            #[::linkme::distributed_slice(
                $crate::world::entity::spawn_placements::SPAWN_PLACEMENTS
            )]
            static REGISTRATION:
                $crate::world::entity::spawn_placements::SpawnPlacementRegistration =
                $crate::world::entity::spawn_placements::SpawnPlacementRegistration {
                    entity_type: $entity_type,
                    predicate: $predicate,
                };
        };
    };
}

/// The `register(...)` lines of `SpawnPlacements.static {}` whose predicate is one of the shared statics.
static SHARED_PREDICATES: &[(SpawnPredicate, &[EntityType])] = &[
    (
        shared::check_mob_spawn_rules,
        &[
            EntityType::ENDER_DRAGON,
            EntityType::IRON_GOLEM,
            EntityType::SNOW_GOLEM,
            EntityType::VILLAGER,
            EntityType::PHANTOM,
            EntityType::SHULKER,
            EntityType::WANDERING_TRADER,
        ],
    ),
    (
        shared::check_animal_spawn_rules,
        &[
            EntityType::CHICKEN,
            EntityType::COW,
            EntityType::DONKEY,
            EntityType::HAPPY_GHAST,
            EntityType::HORSE,
            EntityType::LLAMA,
            EntityType::MULE,
            EntityType::PIG,
            EntityType::SHEEP,
            EntityType::CAT,
            EntityType::PANDA,
            EntityType::TRADER_LLAMA,
        ],
    ),
    (
        shared::check_monster_spawn_rules,
        &[
            EntityType::BOGGED,
            EntityType::CAVE_SPIDER,
            EntityType::CREEPER,
            EntityType::ENDERMAN,
            EntityType::GIANT,
            EntityType::SKELETON,
            EntityType::SPIDER,
            EntityType::WITCH,
            EntityType::WITHER,
            EntityType::WITHER_SKELETON,
            EntityType::CREAKING,
            EntityType::ZOMBIE,
            EntityType::ZOMBIE_HORSE,
            EntityType::ZOMBIE_VILLAGER,
            EntityType::EVOKER,
            EntityType::ILLUSIONER,
            EntityType::RAVAGER,
            EntityType::VEX,
            EntityType::VINDICATOR,
            EntityType::WARDEN,
        ],
    ),
    (
        shared::check_any_light_monster_spawn_rules,
        &[EntityType::BLAZE, EntityType::BREEZE, EntityType::ZOGLIN],
    ),
    (
        shared::check_surface_monsters_spawn_rules,
        &[
            EntityType::CAMEL_HUSK,
            EntityType::HUSK,
            EntityType::PARCHED,
        ],
    ),
    (
        shared::check_surface_water_animal_spawn_rules,
        &[EntityType::COD, EntityType::PUFFERFISH, EntityType::SALMON],
    ),
    (
        shared::check_surface_ageable_water_creature_spawn_rules,
        &[EntityType::DOLPHIN, EntityType::SQUID],
    ),
];

/// `DATA_BY_TYPE`'s predicates (a vanilla registration without an entry here has its mob module still to port).
static PREDICATES: LazyLock<HashMap<EntityType, SpawnPredicate>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    let registrations = SHARED_PREDICATES
        .iter()
        .flat_map(|(predicate, types)| types.iter().map(move |ty| (*ty, *predicate)))
        .chain(
            SPAWN_PLACEMENTS
                .iter()
                .map(|r| (r.entity_type, r.predicate)),
        );
    for (ty, predicate) in registrations {
        assert!(
            ty.data().spawn_placement.is_some(),
            "Spawn predicate registered for {} which has no spawn placement",
            ty.data().name
        );
        if map.insert(ty, predicate).is_some() {
            panic!("Duplicate registration for type {}", ty.data().name);
        }
    }
    map
});

fn predicates() -> &'static HashMap<EntityType, SpawnPredicate> {
    &PREDICATES
}

/// Entity types with a vanilla spawn placement whose predicate is not registered (yet): the mob modules still to port.
pub fn missing_predicates() -> Vec<EntityType> {
    let map = predicates();
    EntityType::iter()
        .filter(|ty| ty.data().spawn_placement.is_some() && !map.contains_key(ty))
        .collect()
}

/// `SpawnPlacements.getPlacementType(type)`: `NO_RESTRICTIONS` for types without a registration.
pub fn get_placement_type(ty: EntityType) -> SpawnPlacementType {
    ty.data()
        .spawn_placement
        .as_ref()
        .map_or(SpawnPlacementType::NoRestrictions, |p| p.placement)
}

/// `SpawnPlacements.isSpawnPositionOk(type, level, pos)`.
pub fn is_spawn_position_ok(
    ty: EntityType,
    level: &dyn ServerLevelAccessor,
    pos: BlockPos,
) -> bool {
    get_placement_type(ty).is_spawn_position_ok(level, pos, Some(ty))
}

/// `SpawnPlacements.getHeightmapType(type)`: `MOTION_BLOCKING_NO_LEAVES` for types without a registration.
pub fn get_heightmap_type(ty: EntityType) -> HeightmapType {
    match ty
        .data()
        .spawn_placement
        .as_ref()
        .map_or(HeightmapTypes::MotionBlockingNoLeaves, |p| p.heightmap)
    {
        HeightmapTypes::WorldSurfaceWg => HeightmapType::WorldSurfaceWg,
        HeightmapTypes::WorldSurface => HeightmapType::WorldSurface,
        HeightmapTypes::OceanFloorWg => HeightmapType::OceanFloorWg,
        HeightmapTypes::OceanFloor => HeightmapType::OceanFloor,
        HeightmapTypes::MotionBlocking => HeightmapType::MotionBlocking,
        HeightmapTypes::MotionBlockingNoLeaves => HeightmapType::MotionBlockingNoLeaves,
    }
}

/// `SpawnPlacements.checkSpawnRules(type, level, reason, pos, random)`.
pub fn check_spawn_rules(
    ty: EntityType,
    level: &dyn ServerLevelAccessor,
    reason: EntitySpawnReason,
    pos: BlockPos,
    random: &mut dyn RandomSource,
) -> bool {
    if !ty.data().allowed_in_peaceful
        && level.get_difficulty() == mc_world::level_storage::Difficulty::Peaceful
    {
        return false;
    }
    if ty.data().spawn_placement.is_none() {
        return true;
    }
    match predicates().get(&ty) {
        Some(predicate) => predicate(ty, level, reason, pos, random),
        // registered in vanilla, the mob itself is not ported yet (such a mob cannot be created either)
        None => false,
    }
}

/// `SpawnPlacements.checkSpawnRules(type, level, reason, pos, level.getRandom())`.
pub fn check_spawn_rules_with_level_random(
    level: &mut dyn ServerLevelAccessor,
    ty: EntityType,
    reason: EntitySpawnReason,
    pos: BlockPos,
) -> bool {
    level.with_random_taken(&mut |level, random| check_spawn_rules(ty, level, reason, pos, random))
}

/// `SpawnPlacementTypes.NO_RESTRICTIONS / IN_WATER / IN_LAVA / ON_GROUND`.
pub trait SpawnPlacementTypeExt: Copy {
    /// `isSpawnPositionOk(level, blockPos, type)`.
    fn is_spawn_position_ok(
        self,
        level: &dyn ServerLevelAccessor,
        pos: BlockPos,
        ty: Option<EntityType>,
    ) -> bool;
    /// `adjustSpawnPosition(level, candidate)`.
    fn adjust_spawn_position(
        self,
        level: &dyn ServerLevelAccessor,
        candidate: BlockPos,
    ) -> BlockPos;
}

impl SpawnPlacementTypeExt for SpawnPlacementType {
    fn is_spawn_position_ok(
        self,
        level: &dyn ServerLevelAccessor,
        pos: BlockPos,
        ty: Option<EntityType>,
    ) -> bool {
        use crate::world::level::block::BlockStateExt as _;
        use crate::world::level::material::FluidStateExt as _;
        use crate::world::level::{BlockGetter as _, LevelReader as _};
        match self {
            SpawnPlacementType::NoRestrictions => true,
            SpawnPlacementType::InWater => {
                if ty.is_some() && level.is_within_world_border(pos) {
                    let above = pos.above();
                    level.get_fluid_state(pos).is_tag("water")
                        && !level.get_block_state(above).is_redstone_conductor()
                } else {
                    false
                }
            }
            SpawnPlacementType::InLava => {
                ty.is_some()
                    && level.is_within_world_border(pos)
                    && level.get_fluid_state(pos).is_tag("lava")
            }
            SpawnPlacementType::OnGround => {
                let Some(ty) = ty.filter(|_| level.is_within_world_border(pos)) else {
                    return false;
                };
                let above = pos.above();
                let below = pos.below();
                let below_state = level.get_block_state(below);
                if !below_state.is_valid_spawn(level, below, ty) {
                    return false;
                }
                is_valid_empty_spawn_block_at(level, pos, ty)
                    && is_valid_empty_spawn_block_at(level, above, ty)
            }
        }
    }

    fn adjust_spawn_position(
        self,
        level: &dyn ServerLevelAccessor,
        candidate: BlockPos,
    ) -> BlockPos {
        use crate::world::level::BlockGetter as _;
        use crate::world::level::block::BlockStateExt as _;
        match self {
            SpawnPlacementType::OnGround => {
                let below = candidate.below();
                if level
                    .get_block_state(below)
                    .is_pathfindable_by(PathComputationType::Land)
                {
                    below
                } else {
                    candidate
                }
            }
            _ => candidate,
        }
    }
}

/// `SpawnPlacementTypes.ON_GROUND.isValidEmptySpawnBlock(level, pos, type)`.
fn is_valid_empty_spawn_block_at(
    level: &dyn ServerLevelAccessor,
    pos: BlockPos,
    ty: EntityType,
) -> bool {
    use crate::world::level::BlockGetter as _;
    let state = level.get_block_state(pos);
    natural_spawner::is_valid_empty_spawn_block(level, pos, state, state.get_fluid_state(), ty)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `SHARED_PREDICATES` entry is a vanilla spawn placement and no type is bound twice.
    #[test]
    fn shared_table_is_consistent() {
        let map = predicates();
        let shared_count: usize = SHARED_PREDICATES.iter().map(|(_, types)| types.len()).sum();
        assert!(map.len() >= shared_count);
        // 83 vanilla registrations: 7 + 12 + 20 + 3 + 3 + 3 + 2 = 50 use shared predicates
        assert_eq!(shared_count, 50);
    }
}
