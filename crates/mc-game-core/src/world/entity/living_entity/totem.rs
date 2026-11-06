//! `LivingEntity.checkTotemDeathProtection(killingDamage)` (the `death_protection` component).

use mc_data::GameEvent;
use mc_protocol::components::values::effects::{DEATH_PROTECTION, USE_EFFECTS};

use super::damage::{as_server_player, broadcast_entity_event};
use crate::server::level::ServerLevel;
use crate::world::damagesource::{DamageSource, DamageTypeTags};
use crate::world::entity::Entity;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::item::ItemStack;

/// The stack in an equipment slot, mutable in place (`getItemInHand(hand)` returns the live stack in vanilla).
pub fn slot_stack_mut(entity: &mut dyn Entity, slot: EquipmentSlot) -> Option<&mut ItemStack> {
    if entity.is_player() {
        return entity
            .player_mut()
            .map(|p| p.inventory.equipment_get_mut(slot));
    }
    entity.living_mut().map(|l| l.equipment.get_mut(slot))
}

/// `LivingEntity.checkTotemDeathProtection(killingDamage)`.
pub fn check_totem_death_protection(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    killing_damage: &DamageSource,
) -> bool {
    if killing_damage.is_tag(level, DamageTypeTags::BYPASSES_INVULNERABILITY) {
        return false;
    }
    let mut protection_item: Option<ItemStack> = None;
    let mut protection = None;
    for slot in [EquipmentSlot::MainHand, EquipmentSlot::OffHand] {
        let Some(stack) = slot_stack_mut(entity, slot) else {
            continue;
        };
        if let Some(p) = stack.get(&DEATH_PROTECTION) {
            protection = Some(p.clone());
            protection_item = Some(stack.copy());
            stack.shrink(1);
            break;
        }
    }
    let Some(protection) = protection else {
        return false;
    };
    let protection_item = protection_item.expect("set together with protection");

    if let Some(player) = as_server_player(entity) {
        player.award_stat(level, crate::stats::Stat::Used(protection_item.item()), 1);
        crate::advancements::triggers::CriteriaTriggers::USED_TOTEM.trigger(
            level,
            player,
            &protection_item,
        );
        // `protectionItem.causeUseVibration(this, ITEM_INTERACT_FINISH)`
        if protection_item
            .get(&USE_EFFECTS)
            .is_some_and(|e| e.interact_vibrations)
        {
            entity.game_event(level, GameEvent::ITEM_INTERACT_FINISH);
        }
    }
    entity.living_mut().expect("living").set_health(1.0);
    for effect in &protection.death_effects {
        crate::world::item::apply_consume_effect(effect, level, entity);
    }
    broadcast_entity_event(level, entity.id(), 35);
    true
}
