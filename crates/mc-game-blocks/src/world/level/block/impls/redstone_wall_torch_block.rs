//! `net.minecraft.world.level.block.RedstoneWallTorchBlock` (extends `RedstoneTorchBlock`).
use super::redstone_torch_block;
use super::wall_torch_block;
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::redstone::Orientation;
use mc_game_core::world::level::signal_getter::has_signal;

/// `RedstoneWallTorchBlock.hasNeighborSignal`.
fn has_neighbor_signal(level: &ServerLevel, pos: BlockPos, state: BlockState) -> bool {
    let opposite = state.get(props::HORIZONTAL_FACING).get_opposite();
    has_signal(level, pos.relative(opposite), opposite)
}

pub struct RedstoneWallTorchBlock;

impl BlockBehaviour for RedstoneWallTorchBlock {
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        let state = helpers::get_state_for_placement(Block::WALL_TORCH, level, context)?;
        Some(block.default_state().set(
            props::HORIZONTAL_FACING,
            state.get(props::HORIZONTAL_FACING),
        ))
    }
    fn on_place(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _old_state: BlockState,
        _moved_by_piston: bool,
    ) {
        redstone_torch_block::notify_neighbors_wall(level, pos, state);
    }
    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        moved_by_piston: bool,
    ) {
        if !moved_by_piston {
            redstone_torch_block::notify_neighbors_wall(level, pos, state);
        }
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        let neighbor_signal = has_neighbor_signal(level, pos, state);
        redstone_torch_block::tick(state, level, pos, neighbor_signal);
    }
    fn neighbor_changed(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _block: Block,
        _orientation: Option<Orientation>,
        _moved_by_piston: bool,
    ) {
        let neighbor_signal = has_neighbor_signal(level, pos, state);
        redstone_torch_block::neighbor_changed(state, level, pos, neighbor_signal);
    }
    fn get_direct_signal(
        &self,
        state: BlockState,
        level: &dyn BlockGetter,
        pos: BlockPos,
        direction: Direction,
    ) -> i32 {
        if direction == Direction::Down {
            state.get_signal(level, pos, direction)
        } else {
            0
        }
    }
    fn own_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        if state.get(props::LIT) { 15 } else { 0 }
    }
    fn get_signal(
        &self,
        state: BlockState,
        level: &dyn BlockGetter,
        pos: BlockPos,
        direction: Direction,
    ) -> i32 {
        if state.get(props::HORIZONTAL_FACING) != direction {
            self.own_signal(state, level, pos)
        } else {
            0
        }
    }
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        wall_torch_block::can_survive(level, pos, state.get(props::HORIZONTAL_FACING))
    }
    fn update_shape(
        &self,
        state: BlockState,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        direction_to_neighbour: Direction,
        _neighbour_pos: BlockPos,
        _neighbour_state: BlockState,
    ) -> BlockState {
        wall_torch_block::update_shape(state, &*level, pos, direction_to_neighbour)
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("RedstoneWallTorchBlock", |_| Box::new(
    RedstoneWallTorchBlock
));
