//! `DyeItem`: dyes a sign's text (`SignApplicator`) and a sheep's wool (`interactLivingEntity`).

use super::{InteractionHand, InteractionResult, ItemBehaviour, ItemStack, SignApplicator};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::block_entity::BlockEntity;
use crate::world::block_entity::sign_block_entity::{SignBlockEntity, SignTextExt, SignTextSlot};
use crate::world::entity::{Entity, ServerPlayer, mob};
use mc_data::SoundEvent;
use mc_protocol::components::DYE;
use mc_protocol::packets::play::SoundSource;

pub struct DyeItem;

impl ItemBehaviour for DyeItem {
    fn interact_living_entity(
        &self,
        level: &mut ServerLevel,
        stack: &mut ItemStack,
        player: &mut ServerPlayer,
        target: &mut dyn Entity,
        _hand: InteractionHand,
    ) -> InteractionResult {
        if target.is_alive()
            && target.sheep_sheared() == Some(false)
            && let Some(dye) = stack.get(&DYE).copied()
            && target.sheep_color() != Some(dye)
        {
            mob::ext::play_sound_from_entity(
                level,
                Some(player.id()),
                target.id(),
                SoundEvent::ITEM_DYE_USE,
                SoundSource::Players,
                1.0,
                1.0,
            );
            target.set_sheep_color(dye);
            stack.shrink(1);
            return InteractionResult::SUCCESS;
        }
        InteractionResult::PASS
    }

    fn as_sign_applicator(&self) -> Option<&dyn SignApplicator> {
        Some(self)
    }
}

impl SignApplicator for DyeItem {
    fn try_apply_to_sign(
        &self,
        level: &mut ServerLevel,
        sign: &mut SignBlockEntity,
        slot: SignTextSlot,
        item: &ItemStack,
        _player: &ServerPlayer,
    ) -> bool {
        let Some(dye) = item.get(&DYE).copied() else {
            return false;
        };
        if sign.update_text(level, |text| text.with_color(dye), slot) {
            level.play_sound(
                None,
                sign.get_block_pos(),
                SoundEvent::ITEM_DYE_USE,
                SoundSource::Blocks,
                1.0,
                1.0,
            );
            true
        } else {
            false
        }
    }
}

register_item_class!("DyeItem", |_| Box::new(DyeItem));
