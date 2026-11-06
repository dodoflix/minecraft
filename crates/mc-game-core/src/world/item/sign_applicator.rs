//! `net.minecraft.world.item.SignApplicator`: items that change a sign's text (dyes, ink sacs, honeycomb).

use super::ItemStack;
use crate::server::level::ServerLevel;
use crate::world::block_entity::sign_block_entity::{
    SignBlockEntity, SignText, SignTextExt, SignTextSlot,
};
use crate::world::entity::ServerPlayer;

pub trait SignApplicator {
    /// `tryApplyToSign(level, sign, slot, item, player)`.
    fn try_apply_to_sign(
        &self,
        level: &mut ServerLevel,
        sign: &mut SignBlockEntity,
        slot: SignTextSlot,
        item: &ItemStack,
        player: &mut ServerPlayer,
    ) -> bool;

    /// `canApplyToSign(text, item, player)`: default `text.hasMessage(player.isTextFilteringEnabled())`.
    fn can_apply_to_sign(&self, text: &SignText, _item: &ItemStack, player: &ServerPlayer) -> bool {
        text.has_message(player.text_filtering_enabled)
    }
}
