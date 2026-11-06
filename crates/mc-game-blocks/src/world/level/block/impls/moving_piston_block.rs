//! `net.minecraft.world.level.block.MovingPistonBlock` (`piston` package): the block entity block of a block in transit.
use crate::world::block_entity::{PistonMovingBlockEntity, moving_piston_collision_shape};
use mc_core::phys::BlockHitResult;
use mc_core::phys::shapes::VoxelShape;
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::world::item::{InteractionResult, ItemStack};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::collision::CollisionContext;

pub struct MovingPistonBlock;

impl BlockBehaviour for MovingPistonBlock {
    fn destroy(&self, level: &mut dyn LevelAccessor, pos: BlockPos, state: BlockState) {
        let relative = pos.relative(state.get(props::FACING).get_opposite());
        let block_state = level.get_block_state(relative);
        if helpers::is_instance_of(block_state, "PistonBaseBlock")
            && block_state.get(props::EXTENDED)
        {
            level.remove_block(relative, false);
        }
    }

    fn use_without_item(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _player: &mut ServerPlayer,
        _hit: &BlockHitResult,
    ) -> InteractionResult {
        if level.get_block_entity_immediate(pos).is_none() {
            level.remove_block(pos, false);
            InteractionResult::CONSUME
        } else {
            InteractionResult::PASS
        }
    }

    /// `getDrops`: the drops of the moved block (nothing without a block entity).
    fn get_drops_state(
        &self,
        _state: BlockState,
        level: &dyn BlockGetter,
        pos: BlockPos,
    ) -> BlockState {
        match level
            .get_block_entity(pos)
            .and_then(|be| be.as_any().downcast_ref::<PistonMovingBlockEntity>())
        {
            Some(entity) => entity.get_moved_state(),
            None => helpers::air(),
        }
    }

    fn get_collision_shape(
        &self,
        _state: BlockState,
        level: &dyn BlockGetter,
        pos: BlockPos,
        _context: &CollisionContext,
    ) -> VoxelShape {
        moving_piston_collision_shape(level, pos)
    }

    fn get_clone_item_stack(
        &self,
        _state: BlockState,
        _level: &ServerLevel,
        _pos: BlockPos,
        _include_data: bool,
    ) -> ItemStack {
        ItemStack::empty()
    }

    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::FACING, mirror)
    }
}

register_block_class!("MovingPistonBlock", |_| Box::new(MovingPistonBlock));
