//! `ReactToBell`.

use super::{BehaviorBuilder, OneShot};
use crate::world::entity::ai::memory::memory_module_type::HEARD_BELL_TIME;
use crate::world::entity::schedule::Activity;

pub fn create() -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let _heard_bell_time = i.present(&HEARD_BELL_TIME);
    i.create(move |c| {
        let nearby_raid = c.level.get_raid_at(c.body().block_position());
        if nearby_raid.is_none() {
            c.body_mut().brain_mut().set_active_activity_if_possible(Activity::HIDE);
        }
        true
    })
}
