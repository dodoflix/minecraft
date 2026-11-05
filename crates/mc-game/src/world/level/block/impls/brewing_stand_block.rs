//! `net.minecraft.world.level.block.BrewingStandBlock`: opens the brewing menu, drops/updates comparators.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::stats::Stats;
use crate::world::block_entity::BrewingStandBlockEntity;
use crate::world::entity::ServerPlayer;
use crate::world::item::InteractionResult;
use mc_core::phys::BlockHitResult;

pub struct BrewingStandBlock;

impl BlockBehaviour for BrewingStandBlock {
    /// `useWithoutItem`: `player.openMenu(blockEntity)` + `INTERACT_WITH_BREWINGSTAND`.
    fn use_without_item(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, _hit: &BlockHitResult) -> InteractionResult {
        if let Some(provider) = BrewingStandBlockEntity::menu_provider(level, pos) {
            player.open_menu(level, &provider);
            player.award_stat_once(level, Stats::INTERACT_WITH_BREWINGSTAND);
        }
        InteractionResult::SUCCESS
    }

    /// `affectNeighborsAfterRemoval`: `Containers.updateNeighboursAfterDestroy(state, level, pos)`.
    fn affect_neighbors_after_removal(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _moved_by_piston: bool) {
        level.update_neighbour_for_output_signal(pos, state.block());
    }

    /// `getAnalogOutputSignal`: `AbstractContainerMenu.getRedstoneSignalFromBlockEntity`.
    fn get_analog_output_signal(&self, _state: BlockState, level: &ServerLevel, pos: BlockPos, _direction: Direction) -> i32 {
        crate::world::inventory::get_redstone_signal_from_block_entity(level.get_block_entity(pos))
    }
}

register_block_class!("BrewingStandBlock", |_| Box::new(BrewingStandBlock));
