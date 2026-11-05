//! `net.minecraft.world.level.block.RedstoneLampBlock`.
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::redstone::Orientation;
use mc_game_core::world::level::signal_getter::has_neighbor_signal;

pub struct RedstoneLampBlock;

impl BlockBehaviour for RedstoneLampBlock {
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        Some(block.default_state().set(props::LIT, has_neighbor_signal(level, context.get_clicked_pos())))
    }
    fn neighbor_changed(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _block: Block, _orientation: Option<Orientation>, _moved_by_piston: bool) {
        let is_lit = state.get(props::LIT);
        if is_lit != has_neighbor_signal(level, pos) {
            if is_lit {
                level.schedule_tick_block(pos, state.block(), 4);
            } else {
                level.set_block_flags(pos, state.set(props::LIT, !is_lit), 2);
            }
        }
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if state.get(props::LIT) && !has_neighbor_signal(level, pos) {
            level.set_block_flags(pos, state.set(props::LIT, false), 2);
        }
    }
}

register_block_class!("RedstoneLampBlock", |_| Box::new(RedstoneLampBlock));
