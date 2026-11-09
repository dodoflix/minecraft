//! `InkSacItem` and `GlowInkSacItem`: turn a sign's glowing text off / on (`SignApplicator`).

use super::{ItemBehaviour, ItemStack, SignApplicator};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::block_entity::BlockEntity;
use crate::world::block_entity::sign_block_entity::{SignBlockEntity, SignTextExt, SignTextSlot};
use crate::world::entity::ServerPlayer;
use mc_data::SoundEvent;
use mc_protocol::packets::play::SoundSource;

/// `InkSacItem` (`glowing = false`) / `GlowInkSacItem` (`glowing = true`).
pub struct InkSacItem {
    glowing: bool,
}

impl ItemBehaviour for InkSacItem {
    fn as_sign_applicator(&self) -> Option<&dyn SignApplicator> {
        Some(self)
    }
}

impl SignApplicator for InkSacItem {
    fn try_apply_to_sign(
        &self,
        level: &mut ServerLevel,
        sign: &mut SignBlockEntity,
        slot: SignTextSlot,
        _item: &ItemStack,
        _player: &ServerPlayer,
    ) -> bool {
        if sign.update_text(level, |text| text.with_glowing_text(self.glowing), slot) {
            let sound = if self.glowing {
                SoundEvent::ITEM_GLOW_INK_SAC_USE
            } else {
                SoundEvent::ITEM_INK_SAC_USE
            };
            level.play_sound(
                None,
                sign.get_block_pos(),
                sound,
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

register_item_class!("InkSacItem", |_| Box::new(InkSacItem { glowing: false }));
register_item_class!("GlowInkSacItem", |_| Box::new(InkSacItem { glowing: true }));
