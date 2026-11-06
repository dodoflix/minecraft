//! `SetHiddenState`.

use super::OneShot;
use super::declarative::BehaviorBuilder;
use super::update_activity_from_schedule;
use crate::world::entity::ai::memory::memory_module_type::{HEARD_BELL_TIME, HIDING_PLACE};

/// `HIDE_TIMEOUT`.
const HIDE_TIMEOUT: i64 = 300;

/// `create(seconds, closeEnoughDist)`.
pub fn create(seconds: i32, close_enough_dist: i32) -> OneShot {
    let stay_hidden_ticks = seconds.wrapping_mul(20);
    let mut ticks_hidden: i32 = 0;
    let mut i = BehaviorBuilder::instance();
    let hiding_place = i.present(&HIDING_PLACE);
    let heard_bell_time = i.present(&HEARD_BELL_TIME);
    i.create(move |c| {
        let time_triggered = *c.get(&heard_bell_time);
        let timed_out_trying_to_hide = time_triggered + HIDE_TIMEOUT <= c.timestamp;
        if ticks_hidden <= stay_hidden_ticks && !timed_out_trying_to_hide {
            let hide_pos = c.get(&hiding_place).pos;
            if hide_pos.closer_than(&c.body.block_position(), f64::from(close_enough_dist)) {
                ticks_hidden = ticks_hidden.wrapping_add(1);
            }
            true
        } else {
            c.erase(&heard_bell_time);
            c.erase(&hiding_place);
            update_activity_from_schedule::update(&*c.level, &mut *c.body);
            ticks_hidden = 0;
            true
        }
    })
}
