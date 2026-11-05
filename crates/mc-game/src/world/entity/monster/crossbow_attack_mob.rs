//! `CrossbowAttackMob.performCrossbowAttack` (the interface itself is [`crate::world::entity::mob::ranged::CrossbowAttackMob`]).

use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, EntityId};
use crate::world::item::crossbow_item::CrossbowItem;
use crate::world::item::projectile_weapon_item::get_weapon_holding_hand;
use mc_data::Item;

/// `performCrossbowAttack(body, crossbowPower)`: fires the crossbow of `body` at `target` (`this.getTarget()`); the
/// caller then runs `onCrossbowAttackPerformed()`.
pub fn perform_crossbow_attack(level: &mut ServerLevel, body: &mut dyn Entity, crossbow_power: f32, uncertainty: f32, target: Option<EntityId>) {
    let hand = get_weapon_holding_hand(&*body, Item::CROSSBOW);
    let slot = hand.as_equipment_slot();
    let mut used_item = body.get_item_by_slot(slot);
    if used_item.item() == Item::CROSSBOW {
        CrossbowItem::singleton().perform_shooting(level, body, hand, &mut used_item, crossbow_power, uncertainty, target);
        // vanilla mutates the stack held in the hand
        if let Some(held) = body.item_by_slot_mut(slot) {
            *held = used_item;
        }
    }
}
