//! `LivingEntity.onEquippedItemBroken(brokenItem, inSlot)` and `stopLocationBasedEffects(previous, inSlot, attributes)`.

use mc_protocol::components::ATTRIBUTE_MODIFIERS;

use super::actors::Actors;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::item::ItemStack;

/// `LivingEntity.entityEventForEquipmentBreak(slot)`.
fn entity_event_for_equipment_break(slot: EquipmentSlot) -> i8 {
    match slot {
        EquipmentSlot::MainHand => 47,
        EquipmentSlot::OffHand => 48,
        EquipmentSlot::Head => 49,
        EquipmentSlot::Chest => 50,
        EquipmentSlot::Feet => 52,
        EquipmentSlot::Legs => 51,
        EquipmentSlot::Body => 65,
        EquipmentSlot::Saddle => 68,
    }
}

pub fn on_equipped_item_broken(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    broken: &ItemStack,
    slot: EquipmentSlot,
) {
    let id = entity.id();
    let packet = mc_protocol::packets::play::ClientboundEntityEventPacket {
        entity_id: id,
        event_id: entity_event_for_equipment_break(slot),
    };
    level.entity_tracker.send_to_tracking_players_and_self(
        id,
        &crate::server::level::level_events::encode_packet(&packet),
    );
    // `previous.forEachModifier(inSlot, (attribute, modifier) -> instance.removeModifier(modifier))`
    if let (Some(modifiers), Some(living)) = (broken.get(&ATTRIBUTE_MODIFIERS), entity.living_mut())
    {
        for entry in &modifiers.modifiers {
            if super::enchantment::slot_group_test(entry.slot, slot)
                && let Some(instance) = living.attributes.get_instance(entry.attribute)
            {
                instance.remove_modifier(&entry.modifier.id);
            }
        }
    }
    // `EnchantmentHelper.forEachModifier(stack, slot, ...)` (the `attributes` effect component)
    let registries = level.registries.clone();
    let mut enchantment_modifiers = Vec::new();
    crate::world::item::enchantment_helper::for_each_modifier(
        &registries,
        broken,
        slot,
        |attribute, modifier| enchantment_modifiers.push((attribute, modifier)),
    );
    if let Some(living) = entity.living_mut() {
        living
            .attributes
            .remove_attribute_modifiers(&enchantment_modifiers);
    }
    crate::world::item::enchantment_helper::stop_location_based_effects_for_slot(
        level,
        &mut Actors::of(entity),
        broken,
        id,
        slot,
    );
}
