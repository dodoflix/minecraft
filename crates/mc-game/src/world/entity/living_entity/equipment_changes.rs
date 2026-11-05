//! `LivingEntity.detectEquipmentUpdates` and friends: `collectEquipmentChanges` (attribute modifiers of the new /
//! old equipment), `handleHandSwap`, `handleEquipmentChanges` (`ClientboundSetEquipmentPacket`),
//! `stopLocationBasedEffects` / `onEquippedItemBroken`, and `ItemStack.forEachModifier`.

use crate::server::level::ServerLevel;
use crate::server::level::level_events::encode_packet;
use crate::world::entity::attributes::{AttributeMap, AttributeModifier};
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::Entity;
use crate::world::item::{ItemStack, ItemStackExt};
use mc_data::Attribute;
use mc_protocol::components::{ATTRIBUTE_MODIFIERS, EquipmentSlotGroup};
use mc_protocol::packets::play::{ClientboundEntityEventPacket, ClientboundSetEquipmentPacket};

/// `EquipmentSlotGroup.test(slot)`.
pub fn equipment_slot_group_test(group: EquipmentSlotGroup, slot: EquipmentSlot) -> bool {
    match group {
        EquipmentSlotGroup::Any => true,
        EquipmentSlotGroup::MainHand => slot == EquipmentSlot::MainHand,
        EquipmentSlotGroup::OffHand => slot == EquipmentSlot::OffHand,
        EquipmentSlotGroup::Hand => slot.get_type() == crate::world::entity::equipment::EquipmentSlotType::Hand,
        EquipmentSlotGroup::Feet => slot == EquipmentSlot::Feet,
        EquipmentSlotGroup::Legs => slot == EquipmentSlot::Legs,
        EquipmentSlotGroup::Chest => slot == EquipmentSlot::Chest,
        EquipmentSlotGroup::Head => slot == EquipmentSlot::Head,
        EquipmentSlotGroup::Armor => slot.is_armor(),
        EquipmentSlotGroup::Body => slot == EquipmentSlot::Body,
        EquipmentSlotGroup::Saddle => slot == EquipmentSlot::Saddle,
    }
}

/// `ItemStack.forEachModifier(slot, consumer)`: the `attribute_modifiers` component, then the enchantments'
/// `attributes` effects.
pub fn for_each_modifier(stack: &ItemStack, slot: EquipmentSlot, mut consumer: impl FnMut(Attribute, &AttributeModifier)) {
    if let Some(modifiers) = stack.get(&ATTRIBUTE_MODIFIERS) {
        for entry in &modifiers.modifiers {
            if equipment_slot_group_test(entry.slot, slot) {
                consumer(entry.attribute, &entry.modifier);
            }
        }
    }
}

/// `stopLocationBasedEffects(previous, inSlot, attributes)` (the attribute part).
fn remove_item_modifiers(attributes: &mut AttributeMap, previous: &ItemStack, slot: EquipmentSlot) {
    for_each_modifier(previous, slot, |attribute, modifier| {
        if let Some(instance) = attributes.get_instance(attribute) {
            instance.remove_modifier(&modifier.id);
        }
    });
}

/// `LivingEntity.onEquippedItemBroken(brokenItem, inSlot)` without the entity event (sent by the caller's level).
pub fn stop_location_based_effects(entity: &mut dyn Entity, previous: &ItemStack, slot: EquipmentSlot) {
    if let Some(living) = entity.living_mut() {
        remove_item_modifiers(&mut living.attributes, previous, slot);
    }
}

/// `detectEquipmentUpdates()`.
pub fn detect_equipment_updates(entity: &mut dyn Entity, level: &mut ServerLevel) {
    let Some(mut changed) = collect_equipment_changes(entity) else { return };
    handle_hand_swap(entity, level, &mut changed);
    if !changed.is_empty() {
        handle_equipment_changes(entity, level, changed);
    }
}

/// `collectEquipmentChanges(lastEquipmentItems)`: the changed slots in `EquipmentSlot` order.
fn collect_equipment_changes(entity: &mut dyn Entity) -> Option<Vec<(EquipmentSlot, ItemStack)>> {
    let mut changed: Option<Vec<(EquipmentSlot, ItemStack)>> = None;
    for slot in EquipmentSlot::VALUES {
        let previous = entity.living()?.last_equipment_items[slot as usize].clone();
        let current = entity.get_item_by_slot(slot);
        // `equipmentHasChanged(previous, current)`: `!ItemStack.matches(current, previous)`
        if !ItemStack::matches(&current, &previous) {
            changed.get_or_insert_with(Vec::new).push((slot, current));
            if !previous.is_empty() {
                stop_location_based_effects(entity, &previous, slot);
            }
        }
    }
    if let Some(changed) = &changed {
        for (slot, current) in changed {
            if !current.is_empty() && !current.is_broken() {
                if let Some(living) = entity.living_mut() {
                    for_each_modifier(current, *slot, |attribute, modifier| {
                        if let Some(instance) = living.attributes.get_instance(attribute) {
                            instance.remove_modifier(&modifier.id);
                            instance.add_transient_modifier(modifier.clone());
                        }
                    });
                }
            }
        }
    }
    changed
}

/// `handleHandSwap(changedItems)`.
fn handle_hand_swap(entity: &mut dyn Entity, level: &mut ServerLevel, changed: &mut Vec<(EquipmentSlot, ItemStack)>) {
    let get = |changed: &Vec<(EquipmentSlot, ItemStack)>, slot: EquipmentSlot| changed.iter().find(|(s, _)| *s == slot).map(|(_, st)| st.clone());
    let (Some(main), Some(off)) = (get(changed, EquipmentSlot::MainHand), get(changed, EquipmentSlot::OffHand)) else { return };
    let last = &entity.living().expect("living").last_equipment_items;
    if ItemStack::matches(&main, &last[EquipmentSlot::OffHand as usize]) && ItemStack::matches(&off, &last[EquipmentSlot::MainHand as usize]) {
        let packet = ClientboundEntityEventPacket { entity_id: entity.id(), event_id: 55 };
        level.entity_tracker.send_to_tracking_players(entity.id(), &encode_packet(&packet));
        changed.retain(|(s, _)| *s != EquipmentSlot::MainHand && *s != EquipmentSlot::OffHand);
        let last = &mut entity.living_mut().expect("living").last_equipment_items;
        last[EquipmentSlot::MainHand as usize] = main.copy();
        last[EquipmentSlot::OffHand as usize] = off.copy();
    }
}

/// `handleEquipmentChanges(changedItems)` + `updatePlayersWithNewEquipment`.
fn handle_equipment_changes(entity: &mut dyn Entity, level: &mut ServerLevel, changed: Vec<(EquipmentSlot, ItemStack)>) {
    let mut to_send = Vec::with_capacity(changed.len());
    for (slot, new_item) in changed {
        let stored = new_item.copy();
        to_send.push((slot.to_protocol(), stored.clone()));
        entity.living_mut().expect("living").last_equipment_items[slot as usize] = stored;
    }
    let packet = ClientboundSetEquipmentPacket { entity: entity.id(), slots: to_send };
    level.entity_tracker.send_to_tracking_players(entity.id(), &encode_packet(&packet));
}
