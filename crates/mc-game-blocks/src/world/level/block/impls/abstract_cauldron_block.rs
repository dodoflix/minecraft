//! `net.minecraft.world.level.block.AbstractCauldronBlock` (shared by `CauldronBlock`, `LayeredCauldronBlock` and
//! `LavaCauldronBlock`, which call these from their `BlockBehaviour` impls).
use super::cauldron_interactions::{self, CauldronKind};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::block::impls::prelude::*;

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
