//! `CarvedPumpkinBlock.clearPatternBlocks` / `updatePatternBlocks` (also used by `WitherSkullBlock.checkSpawn`) and the
//! entity creation and `SUMMONED_ENTITY` trigger shared by the golem/wither summoning.

use super::pattern::BlockPatternMatch;
use crate::advancements::triggers::{CriteriaTriggers, fire_with_entity};
use crate::server::level::ServerLevel;
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::level::LevelAccessor;
use mc_data::EntityType;
use mc_data::block::Block;
use mc_data::enums::EntitySpawnReason;

/// `clearPatternBlocks(level, match)`: every block of the first aisle becomes air (flags 2) with the destroy particles of
/// the block state `BlockInWorld` has cached (a position the pattern never read is read after the block was cleared).
pub fn clear_pattern_blocks(level: &mut ServerLevel, m: &mut BlockPatternMatch) {
    for x in 0..m.get_width() {
        for y in 0..m.get_height() {
            let block = m.get_block(x, y, 0);
            let pos = block.get_pos();
            level.set_block_flags(pos, super::helpers::air(), 2);
            let state = block
                .get_state(&*level)
                .expect("the position is loaded: it was just set");
            level.level_event(None, 2001, pos, state.id() as i32);
        }
    }
}

/// `updatePatternBlocks(level, match)`: `level.updateNeighborsAt(pos, Blocks.AIR)` for every block of the first aisle.
pub fn update_pattern_blocks(level: &mut ServerLevel, m: &mut BlockPatternMatch) {
    for x in 0..m.get_width() {
        for y in 0..m.get_height() {
            let pos = m.get_block(x, y, 0).get_pos();
            level.update_neighbors_at(pos, Block::AIR);
        }
    }
}

/// `EntityType.create(level, TRIGGERED)` for the summoned golem/wither: the entity, not yet added to the level.
pub fn create_triggered(
    level: &ServerLevel,
    ty: EntityType,
) -> Option<Box<dyn crate::world::entity::Entity>> {
    factory::create(
        ty,
        level,
        EntitySpawnRequest {
            reason: EntitySpawnReason::Triggered,
            ignore_checks: false,
        },
    )
}

/// `level.addFreshEntity(entity)`: the entity's id when it was added.
pub fn add_fresh_entity(
    level: &mut ServerLevel,
    entity: Box<dyn crate::world::entity::Entity>,
) -> Option<EntityId> {
    let id = entity.id();
    level.add_fresh_entity(entity).then_some(id)
}

/// `CriteriaTriggers.SUMMONED_ENTITY.trigger(player, entity)` for the entity with id `entity_id` (which must be in the
/// level), reading both from the level.
fn award_summoned_entity(level: &mut ServerLevel, player: &mut ServerPlayer, entity_id: EntityId) {
    let Some(entity) = level.get_entity(entity_id) else {
        return;
    };
    let matches = CriteriaTriggers::SUMMONED_ENTITY.matches(&*level, &*player, entity);
    matches.award(level, player);
}

/// `level.getEntitiesOfClass(ServerPlayer.class, entity.getBoundingBox().inflate(range))` +
/// `CriteriaTriggers.SUMMONED_ENTITY.trigger(player, entity)` for the not yet added `entity`. When one of the players is
/// running (taken out of the level: the one whose placement summoned the entity) every award waits until it is put back,
/// to keep vanilla's order.
pub fn trigger_summoned_entity(level: &mut ServerLevel, entity: &dyn Entity, range: f64) {
    let search = entity.get_bounding_box().inflate_scalar(range);
    let players = level.get_players_in_aabb(&search);
    let Some(running) = players
        .iter()
        .copied()
        .find(|&id| level.get_entity(id).is_none())
    else {
        for id in players {
            fire_with_entity(level, id, entity, |level, player, entity| {
                CriteriaTriggers::SUMMONED_ENTITY.matches(level, player, entity)
            });
        }
        return;
    };
    let entity_id = entity.id();
    level.with_entity_or_defer(running, move |level, running_entity| {
        for id in players {
            if id == running {
                if let Some(player) = running_entity.as_any_mut().downcast_mut::<ServerPlayer>() {
                    award_summoned_entity(level, player, entity_id);
                }
            } else {
                level.with_player(id, |level, player| {
                    award_summoned_entity(level, player, entity_id);
                });
            }
        }
    });
}

/// `CarvedPumpkinBlock.spawnGolemInWorld(level, match, golem, spawnPos)`.
pub fn spawn_golem_in_world(
    level: &mut ServerLevel,
    m: &mut BlockPatternMatch,
    mut golem: Box<dyn Entity>,
    spawn_pos: mc_core::pos::BlockPos,
) {
    clear_pattern_blocks(level, m);
    golem.snap_to(
        f64::from(spawn_pos.x()) + 0.5,
        f64::from(spawn_pos.y()) + 0.05,
        f64::from(spawn_pos.z()) + 0.5,
        0.0,
        0.0,
    );
    // vanilla triggers after `addFreshEntity`; the trigger only reads the entity, so the order is not observable
    trigger_summoned_entity(level, &*golem, 5.0);
    add_fresh_entity(level, golem);
    update_pattern_blocks(level, m);
}
