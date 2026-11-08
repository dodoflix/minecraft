//! The `Mob`-only calls the block spawners make on a freshly created entity, in vanilla's order
//! (`Mob.checkSpawnRules`, `checkSpawnObstruction`, `finalizeSpawn`, `setPersistenceRequired`, `equip`, `spawnAnim`,
//! `dropPreservedEquipment`) and `SpawnPlacements.checkSpawnRules`. The per-mob behaviour behind each call
//! (`finalizeSpawn`, `checkSpawnRules`, equipment) lives with the mob classes.

use crate::server::level::ServerLevel;
use crate::world::entity::equipment_table::EquipmentTable;
use crate::world::entity::spawn_placements;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use mc_core::pos::BlockPos;
use mc_data::EntityType;
use mc_data::enums::EntitySpawnReason;

/// `SpawnPlacements.checkSpawnRules(type, level, reason, pos, level.getRandom())`.
pub fn check_spawn_placements(
    level: &mut ServerLevel,
    ty: EntityType,
    reason: EntitySpawnReason,
    pos: BlockPos,
) -> bool {
    spawn_placements::check_spawn_rules_with_level_random(level, ty, reason, pos)
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
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    reason: EntitySpawnReason,
    steps: &MobSpawnSteps,
) -> bool {
    if !entity.is_mob() {
        return true;
    }
    if steps.check_spawn_rules && !entity.check_spawn_rules(&*level, reason) {
        return false;
    }
    if !entity.check_spawn_obstruction(&*level) {
        return false;
    }
    if steps.finalize_spawn {
        let difficulty =
            ServerLevelAccessor::get_current_difficulty_at(&*level, entity.block_position());
        entity.finalize_spawn(level, difficulty, reason, None);
    }
    if steps.set_persistence_required {
        entity.set_persistence_required();
    }
    if let Some(table) = steps.equipment {
        entity.equip(level, table);
    }
    true
}

/// `mob.spawnAnim()` on an entity that is already in the level (server side: entity event 20).
pub fn spawn_anim(level: &mut ServerLevel, entity: EntityId) {
    if level.get_entity(entity).is_some_and(|e| e.is_mob()) {
        level.broadcast_entity_event(entity, 20);
    }
}

/// `mob.dropPreservedEquipment(level)` (a trial spawner becoming ominous discards its mobs).
pub fn drop_preserved_equipment(level: &mut ServerLevel, entity: EntityId) {
    level.with_entity(entity, |level, e| {
        if e.is_mob() {
            e.drop_preserved_equipment(level);
        }
    });
}
