//! SEAM(Mob): the `Mob`-only calls the block spawners make on a freshly created entity, in vanilla's order
//! (`Mob.checkSpawnRules`, `checkSpawnObstruction`, `finalizeSpawn`, `setPersistenceRequired`, `equip`, `spawnAnim`,
//! `dropPreservedEquipment`) and `SpawnPlacements.checkSpawnRules`. The mob spine and the spawn framework do not
//! exist yet, so no entity is a `Mob` and every entity passes these checks untouched. Once they exist, replace the
//! bodies with the `Entity`-trait hooks and `SpawnPlacements::check_spawn_rules`; callers do not change.

use crate::server::level::ServerLevel;
use crate::world::entity::equipment_table::EquipmentTable;
use crate::world::entity::{Entity, EntityId};
use mc_core::pos::BlockPos;
use mc_data::EntityType;
use mc_data::enums::EntitySpawnReason;

/// `SpawnPlacements.checkSpawnRules(type, level, reason, pos, random)`.
pub fn check_spawn_placements(
    _level: &mut ServerLevel,
    _ty: EntityType,
    _reason: EntitySpawnReason,
    _pos: BlockPos,
) -> bool {
    true
}

/// Which `Mob` steps a spawner runs between loading the entity and adding it.
pub struct MobSpawnSteps<'a> {
    /// `mob.checkSpawnRules(level, reason)` (`BaseSpawner` without custom spawn rules only).
    pub check_spawn_rules: bool,
    /// `mob.finalizeSpawn(level, level.getCurrentDifficultyAt(pos), reason, null)` (the entity tag is just `{id}`).
    pub finalize_spawn: bool,
    /// `mob.setPersistenceRequired()` (trial spawners).
    pub set_persistence_required: bool,
    pub equipment: Option<&'a EquipmentTable>,
}

/// The `if (entity instanceof Mob mob) { ... }` block of both spawners. `false` = the spawn attempt is rejected
/// (`continue` / `Optional.empty()`). Order: rules, obstruction, finalize, persistence, equipment.
pub fn prepare_mob(
    _entity: &mut dyn Entity,
    _level: &mut ServerLevel,
    _reason: EntitySpawnReason,
    _steps: &MobSpawnSteps,
) -> bool {
    true
}

/// `mob.spawnAnim()` on an entity that is already in the level.
pub fn spawn_anim(_level: &mut ServerLevel, _entity: EntityId) {}

/// `mob.dropPreservedEquipment(level)` (a trial spawner becoming ominous discards its mobs).
pub fn drop_preserved_equipment(_level: &mut ServerLevel, _entity: EntityId) {}
