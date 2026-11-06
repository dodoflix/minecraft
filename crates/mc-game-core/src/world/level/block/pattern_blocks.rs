//! `CarvedPumpkinBlock.clearPatternBlocks` / `updatePatternBlocks` (also used by `WitherSkullBlock.checkSpawn`) and the
//! entity creation shared by the golem/wither summoning.
//!
//! Not implemented: `CarvedPumpkinBlock.onPlace` (`trySpawnGolem`) is not ported, so a carved pumpkin or jack o'lantern
//! never builds a snow or iron golem; only the wither pattern uses these helpers.

use super::pattern::BlockPatternMatch;
use crate::server::level::ServerLevel;
use crate::world::entity::EntityId;
use crate::world::entity::factory::{self, EntitySpawnRequest};
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
