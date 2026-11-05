//! `net.minecraft.world.level.block.AbstractCauldronBlock` (shared by `CauldronBlock`, `LayeredCauldronBlock` and
//! `LavaCauldronBlock`, which call these from their `BlockBehaviour` impls).
use super::cauldron_interactions::{self, CauldronKind};
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::entity::ServerPlayer;
use crate::world::item::{InteractionHand, InteractionResult, ItemStack};

/// `AbstractCauldronBlock.useItemOn`: `interactions.get(itemStack).interact(..)`.
pub fn use_item_on(kind: CauldronKind, stack: &ItemStack, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
    cauldron_interactions::interact(kind, state, level, pos, player, hand, stack)
}
