//! `BlockPredicate.matches(level, pos)` (the codec and the state/block-entity-data tests live in mc-protocol).

use crate::server::level::ServerLevel;
use crate::world::block_entity::BlockEntity;
use crate::world::level::{BlockGetter, LevelReader};
use mc_core::pos::BlockPos;
use mc_protocol::components::BlockPredicate;

pub trait BlockPredicateLevelExt {
    /// `matches(ServerLevel, BlockPos)`.
    fn matches(&self, level: &ServerLevel, pos: BlockPos) -> bool;
    /// `matchesBlockEntity(level, blockEntity)`: the entity's `saveWithFullMetadata` tag and `collectComponents()`.
    fn matches_block_entity_of(&self, block_entity: Option<&dyn BlockEntity>) -> bool;
}

impl BlockPredicateLevelExt for BlockPredicate {
    fn matches(&self, level: &ServerLevel, pos: BlockPos) -> bool {
        if !level.is_loaded(pos) || !self.matches_state(level.get_block_state(pos)) {
            return false;
        }
        !self.will_match_block_entity() || self.matches_block_entity_of(level.get_block_entity(pos))
    }

    // shortcut: `collectComponents()` is the stored `components` map; block entities whose `collectImplicitComponents`
    // adds components (custom name, container contents, ...) are not reflected, upgrade with the block entity slice.
    fn matches_block_entity_of(&self, block_entity: Option<&dyn BlockEntity>) -> bool {
        let tag = block_entity.map(|e| e.save_with_full_metadata());
        self.matches_block_entity(block_entity.zip(tag.as_ref()).map(|(e, t)| (t, &e.base().components)))
    }
}
