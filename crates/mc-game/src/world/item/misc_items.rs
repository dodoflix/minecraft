//! `SpyglassItem`, `ShieldItem` (the item classes of the books/instruments/banners/misc slice that only need the item API).

use super::{InteractionHand, InteractionResult, ItemBehaviour, ItemStack, PlainItem};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::stats::Stat;
use crate::world::entity::{Entity, ServerPlayer};
use mc_data::SoundEvent;
use mc_protocol::components::{BASE_COLOR, ItemUseAnimation};
use mc_protocol::packets::play::SoundSource;
use mc_text::Component;

/// `Player.playSound(sound, volume, pitch)`: heard by everybody except the player itself.
pub fn player_play_sound(level: &mut ServerLevel, player: &dyn Entity, sound: SoundEvent, volume: f32, pitch: f32) {
    let p = player.position();
    level.play_sound_at(Some(player.id()), p.x, p.y, p.z, sound, SoundSource::Players, volume, pitch);
}

/// `Entity.playSound(sound, volume, pitch)` / `Player.playSound` of a `LivingEntity` that stops using its item.
fn entity_stop_sound(level: &mut ServerLevel, entity: &dyn Entity, sound: SoundEvent) {
    if entity.is_player() {
        player_play_sound(level, entity, sound, 1.0, 1.0);
    } else {
        super::entity_play_sound(level, entity, sound, 1.0, 1.0);
    }
}

struct SpyglassItem;

impl ItemBehaviour for SpyglassItem {
    fn get_use_duration(&self, _stack: &ItemStack) -> i32 {
        1200
    }

    fn get_use_animation(&self, _stack: &ItemStack) -> ItemUseAnimation {
        ItemUseAnimation::Spyglass
    }

    fn use_item(&self, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
        player_play_sound(level, player, SoundEvent::ITEM_SPYGLASS_USE, 1.0, 1.0);
        let item = player.get_item_in_hand(hand).item();
        player.award_stat_once(level, Stat::Used(item));
        // `ItemUtils.startUsingInstantly`
        player.start_using_item(level, hand);
        InteractionResult::CONSUME
    }

    fn finish_using_item(&self, stack: &mut ItemStack, level: &mut ServerLevel, entity: &mut dyn Entity) -> ItemStack {
        entity_stop_sound(level, entity, SoundEvent::ITEM_SPYGLASS_STOP_USING);
        stack.clone()
    }

    fn release_using(&self, _stack: &mut ItemStack, level: &mut ServerLevel, entity: &mut dyn Entity, _remaining_time: i32) -> bool {
        entity_stop_sound(level, entity, SoundEvent::ITEM_SPYGLASS_STOP_USING);
        true
    }
}

register_item_class!("SpyglassItem", |_| Box::new(SpyglassItem));

/// `ShieldItem.getName`: `<descriptionId>.<base color>` when the shield has a `base_color`.
struct ShieldItem {
    description_id: &'static str,
}

impl ItemBehaviour for ShieldItem {
    fn get_name(&self, stack: &ItemStack) -> Component {
        match stack.get(&BASE_COLOR) {
            Some(color) => Component::translatable(format!("{}.{}", self.description_id, color.name()), Vec::new()),
            None => PlainItem.get_name(stack),
        }
    }
}

register_item_class!("ShieldItem", |item| Box::new(ShieldItem { description_id: item.data().description_id }));
