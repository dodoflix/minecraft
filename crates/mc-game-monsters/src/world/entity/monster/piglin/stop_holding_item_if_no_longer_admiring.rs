//! `StopHoldingItemIfNoLongerAdmiring`.

use super::piglin::Piglin;
use super::piglin_ai;
use crate::world::entity::ai::behavior::{BehaviorBuilder, OneShot};
use crate::world::entity::ai::memory::memory_module_type::ADMIRING_ITEM;
use crate::world::entity::equipment::EquipmentSlot;
use mc_protocol::components::BLOCKS_ATTACKS;

pub fn create() -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let _admiring = i.absent(&ADMIRING_ITEM);
    i.create(move |c| {
        let off_hand = c.body.get_item_by_slot(EquipmentSlot::OffHand);
        if !off_hand.is_empty() && off_hand.get(&BLOCKS_ATTACKS).is_none() {
            let (level, body) = (&mut *c.level, &mut *c.body);
            piglin_ai::stop_holding_off_hand_item(level, body.as_any_mut().downcast_mut::<Piglin>().expect("a piglin"), true);
            true
        } else {
            false
        }
    })
}
