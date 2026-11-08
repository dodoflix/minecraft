//! `net.minecraft.world.level.block.AbstractFurnaceBlock` with `FurnaceBlock`, `SmokerBlock` and `BlastFurnaceBlock`:
//! placement facing, the interaction stat, the comparator signal, `rotate` and `mirror`. The block entity ticks itself
//! (`getTicker`), see `abstract_furnace_block_entity`.
//!
//! Not implemented: `useWithoutItem` opens the menu through the block entity's `menu_provider`, which stays `None` until
//! the furnace, smoker and blast furnace menus exist; the interaction stat is awarded either way, like vanilla.
use crate::world::block_entity::abstract_furnace_block_entity::{
    AbstractFurnaceBlockEntity, FurnaceKind,
};
use mc_core::phys::BlockHitResult;
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::stats::stats::Stats;
use mc_game_core::world::container::get_redstone_signal_from_block_entity;
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::world::item::{BlockPlaceContext, InteractionResult};
use mc_game_core::world::level::block::impls::prelude::*;

pub struct AbstractFurnaceBlock {
    kind: FurnaceKind,
}

impl BlockBehaviour for AbstractFurnaceBlock {
    fn get_state_for_placement(
        &self,
        block: Block,
        _level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        Some(block.default_state().set(
            props::HORIZONTAL_FACING,
            context.get_horizontal_direction().get_opposite(),
        ))
    }

    /// `useWithoutItem` -> `openContainer(level, pos, player)` of the concrete block.
    fn use_without_item(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        _hit: &BlockHitResult,
    ) -> InteractionResult {
        let Some(be) = level.get_block_entity(pos) else {
            return InteractionResult::SUCCESS;
        };
        let is_own_kind = be
            .as_any()
            .downcast_ref::<AbstractFurnaceBlockEntity>()
            .is_some_and(|furnace| furnace.kind() == self.kind);
        if is_own_kind {
            if let Some(provider) = be.menu_provider() {
                player.open_menu(level, &*provider);
            }
            player.award_stat_once(
                &*level,
                match self.kind {
                    FurnaceKind::Furnace => Stats::INTERACT_WITH_FURNACE,
                    FurnaceKind::BlastFurnace => Stats::INTERACT_WITH_BLAST_FURNACE,
                    FurnaceKind::Smoker => Stats::INTERACT_WITH_SMOKER,
                },
            );
        }
        InteractionResult::SUCCESS
    }

    /// `affectNeighborsAfterRemoval`: `Containers.updateNeighboursAfterDestroy(state, level, pos)`.
    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        level.update_neighbour_for_output_signal(pos, state.block());
    }

    /// `getAnalogOutputSignal`: `AbstractContainerMenu.getRedstoneSignalFromBlockEntity`.
    fn get_analog_output_signal(
        &self,
        _state: BlockState,
        level: &ServerLevel,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        get_redstone_signal_from_block_entity(level.get_block_entity(pos))
    }

    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }

    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("FurnaceBlock", |_| Box::new(AbstractFurnaceBlock {
    kind: FurnaceKind::Furnace
}));
register_block_class!("BlastFurnaceBlock", |_| Box::new(AbstractFurnaceBlock {
    kind: FurnaceKind::BlastFurnace
}));
register_block_class!("SmokerBlock", |_| Box::new(AbstractFurnaceBlock {
    kind: FurnaceKind::Smoker
}));
