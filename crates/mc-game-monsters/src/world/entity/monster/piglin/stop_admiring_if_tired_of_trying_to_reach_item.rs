//! `StopAdmiringIfTiredOfTryingToReachItem`.

use mc_game_core::world::entity::ai::behavior::{BehaviorBuilder, OneShot};
use mc_game_core::world::entity::ai::memory::memory_module_type::{ADMIRING_ITEM, DISABLE_WALK_TO_ADMIRE_ITEM, NEAREST_VISIBLE_WANTED_ITEM, TIME_TRYING_TO_REACH_ADMIRE_ITEM};
use mc_game_core::world::entity::equipment::EquipmentSlot;

pub fn create(max_time_to_reach_item: i32, disable_time: i32) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let admiring = i.present(&ADMIRING_ITEM);
    let _nearest_visible = i.present(&NEAREST_VISIBLE_WANTED_ITEM);
    let time = i.registered(&TIME_TRYING_TO_REACH_ADMIRE_ITEM);
    let disable_walk = i.registered(&DISABLE_WALK_TO_ADMIRE_ITEM);
    i.create(move |c| {
        if !c.body.get_item_by_slot(EquipmentSlot::OffHand).is_empty() {
            return false;
        }
        match c.try_get(&time).copied() {
            None => c.set(&time, 0),
            Some(time_trying_to_reach) => {
                if time_trying_to_reach > max_time_to_reach_item {
                    c.erase(&admiring);
                    c.erase(&time);
                    c.set_with_expiry(&disable_walk, true, i64::from(disable_time));
                } else {
                    c.set(&time, time_trying_to_reach.wrapping_add(1));
                }
            }
        }
        true
    })
}
