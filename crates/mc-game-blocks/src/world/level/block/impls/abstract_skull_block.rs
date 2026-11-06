//! `net.minecraft.world.level.block.AbstractSkullBlock`: `POWERED` handling shared by every skull class.
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::props;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::BlockGetter;
use mc_game_core::world::level::signal_getter::has_neighbor_signal;

/// `getStateForPlacement`: `defaultBlockState().setValue(POWERED, level.hasNeighborSignal(pos))`.
pub fn get_state_for_placement(block: Block, level: &dyn BlockGetter, pos: BlockPos) -> BlockState {
    block
        .default_state()
        .set(props::POWERED, has_neighbor_signal(level, pos))
}

/// `neighborChanged`: follows the redstone signal into `POWERED` (flags 2).
pub fn neighbor_changed(state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    use mc_game_core::world::level::LevelAccessor;
    let signal = has_neighbor_signal(&*level, pos);
    if signal != state.get(props::POWERED) {
        level.set_block_flags(pos, state.set(props::POWERED, signal), 2);
    }
}
