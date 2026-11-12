//! `SpyglassItem`, `ShieldItem` (the item classes of the books/instruments/banners/misc items that only need the item API).

use super::{
    InteractionHand, InteractionResult, ItemBehaviour, ItemStack, PlainItem, player_play_sound,
};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::stats::Stat;
use crate::world::entity::{Entity, ServerPlayer};
use mc_data::SoundEvent;
use mc_protocol::components::{BASE_COLOR, ItemUseAnimation};
use mc_text::Component;

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

    fn use_item(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        player_play_sound(level, player, SoundEvent::ITEM_SPYGLASS_USE, 1.0, 1.0);
        let item = player.get_item_in_hand(hand).item();
        player.award_stat_once(level, Stat::Used(item));
        // `ItemUtils.startUsingInstantly`
        player.start_using_item(level, hand);
        InteractionResult::CONSUME
    }

    fn finish_using_item(
        &self,
        stack: &mut ItemStack,
        level: &mut ServerLevel,
        entity: &mut dyn Entity,
    ) -> ItemStack {
        entity_stop_sound(level, entity, SoundEvent::ITEM_SPYGLASS_STOP_USING);
        stack.clone()
    }

    fn release_using(
        &self,
        _stack: &mut ItemStack,
        level: &mut ServerLevel,
        entity: &mut dyn Entity,
        _remaining_time: i32,
    ) -> bool {
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
            Some(color) => Component::translatable(
                format!("{}.{}", self.description_id, color.name()),
                Vec::new(),
            ),
            None => PlainItem.get_name(stack),
        }
    }
}

register_item_class!("ShieldItem", |item| Box::new(ShieldItem {
    description_id: item.data().description_id
}));

/// `AirItem.getName`: the `item_name` of the item's own default components (an empty stack has none).
struct AirItem;

impl ItemBehaviour for AirItem {
    fn get_name(&self, _stack: &ItemStack) -> Component {
        {
            use mc_protocol::components::map::DataComponentGetter as _;
            mc_protocol::components::item_stack::item_prototype(mc_data::Item::AIR)
                .get(&mc_protocol::components::ITEM_NAME)
                .cloned()
                .unwrap_or_else(|| Component::literal(""))
        }
    }
}

register_item_class!("AirItem", |_| Box::new(AirItem));
