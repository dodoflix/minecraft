//! `StopAdmiringIfItemTooFarAway`.

use crate::world::entity::ai::behavior::{BehaviorBuilder, OneShot};
use crate::world::entity::ai::memory::memory_module_type::{ADMIRING_ITEM, NEAREST_VISIBLE_WANTED_ITEM};
use crate::world::entity::ai::sensing::closer_than;
use crate::world::entity::equipment::EquipmentSlot;

pub fn create(max_distance_to_item: i32) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let admiring = i.present(&ADMIRING_ITEM);
    let nearest = i.registered(&NEAREST_VISIBLE_WANTED_ITEM);
    i.create(move |c| {
        if !c.body.get_item_by_slot(EquipmentSlot::OffHand).is_empty() {
            return false;
        }
        let near_item = c.try_get(&nearest).and_then(|r| c.level.get_entity(r.0)).is_some_and(|item| closer_than(item, &*c.body, f64::from(max_distance_to_item)));
        if near_item {
            false
        } else {
            c.erase(&admiring);
            true
        }
    })
}
