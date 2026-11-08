//! `net.minecraft.world.level.block.AbstractCauldronBlock` (shared by `CauldronBlock`, `LayeredCauldronBlock` and
//! `LavaCauldronBlock`, which call these from their `BlockBehaviour` impls).
use super::cauldron_interactions::{self, CauldronKind};
use super::pointed_dripstone_block::{
    find_stalactite_tip_above_cauldron, get_cauldron_fill_fluid_type,
};
use mc_core::phys::shapes::VoxelShape;
use mc_data::fluid::Fluid;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::shapes::column;

/// `AbstractCauldronBlock.useItemOn`: `interactions.get(itemStack).interact(..)`.
pub fn use_item_on(
    kind: CauldronKind,
    stack: &ItemStack,
    state: BlockState,
    level: &mut ServerLevel,
    pos: BlockPos,
    player: &mut ServerPlayer,
    hand: InteractionHand,
) -> InteractionResult {
    cauldron_interactions::interact(kind, state, level, pos, player, hand, stack)
}

/// `AbstractCauldronBlock.SHAPE_INSIDE`, also the interaction shape of every cauldron.
pub fn interaction_shape() -> VoxelShape {
    column(12.0, 4.0, 16.0)
}

/// `AbstractCauldronBlock.tick`: fill from a dripstone above (the dripstone side scheduled this tick).
pub fn tick(this: &dyn BlockBehaviour, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    if let Some(stalactite_pos) = find_stalactite_tip_above_cauldron(level, pos) {
        let fluid = get_cauldron_fill_fluid_type(level, stalactite_pos);
        if fluid != Fluid::EMPTY && this.can_receive_stalactite_drip(fluid) {
            this.receive_stalactite_drip(state, level, pos, fluid);
        }
    }
}
