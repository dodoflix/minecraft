//! `DyeItem`: dyes a sign's text (`SignApplicator`).
//! Not implemented: `interactLivingEntity` (dyeing a sheep; needs `Sheep.setColor` through an `Entity` hook).

use super::{ItemBehaviour, ItemStack, SignApplicator};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::block_entity::BlockEntity;
use crate::world::block_entity::sign_block_entity::{SignBlockEntity, SignTextExt, SignTextSlot};
use crate::world::entity::ServerPlayer;
use mc_data::SoundEvent;
use mc_protocol::components::DYE;
use mc_protocol::packets::play::SoundSource;

pub struct DyeItem;

impl ItemBehaviour for DyeItem {
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
