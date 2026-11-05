//! `net.minecraft.world.level.block.RepeaterBlock` (extends `DiodeBlock`).
use mc_game_core::{register_block_class, update_shape};
use mc_game_core::world::level::block::impls::diode_block::{self, Diode, diode_behaviour};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::InteractionResult;

pub struct RepeaterBlock;

impl Diode for RepeaterBlock {
    fn get_delay(&self, state: BlockState) -> i32 {
        state.get(props::DELAY) * 2
    }
    /// `isLocked`: `getAlternateSignal(level, pos, state) > 0`.
    fn is_locked(&self, level: &dyn BlockGetter, pos: BlockPos, state: BlockState) -> bool {
        diode_block::get_alternate_signal(self, level, pos, state) > 0
    }
    fn side_input_diodes_only(&self) -> bool {
        true
    }
}

impl BlockBehaviour for RepeaterBlock {
    diode_behaviour!();
    fn use_without_item(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut mc_game_core::world::entity::ServerPlayer,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        if !player.player.abilities.may_build {
            InteractionResult::PASS
        } else {
            level.set_block_and_update(pos, state.cycle(props::DELAY));
            InteractionResult::SUCCESS
        }
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &mc_game_core::world::item::BlockPlaceContext) -> Option<BlockState> {
        let state = diode_block::get_state_for_placement(block, context);
        Some(state.set(props::LOCKED, self.is_locked(&*level, context.get_clicked_pos(), state)))
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Down && !diode_block::can_survive_on(&*level, neighbour_pos, neighbour_state) {
            air()
        } else if direction.get_axis() != state.get(props::HORIZONTAL_FACING).get_axis() {
            // `!level.isClientSide()` is always true for the server levels
            state.set(props::LOCKED, self.is_locked(&*level, pos, state))
        } else {
            state
        }
    });
    fn should_redstone_wire_connect_to(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos, direction: Option<Direction>) -> bool {
        let repeater_direction = state.get(props::HORIZONTAL_FACING);
        Some(repeater_direction) == direction || Some(repeater_direction.get_opposite()) == direction
    }
}


register_block_class!("RepeaterBlock", |_| Box::new(RepeaterBlock));
