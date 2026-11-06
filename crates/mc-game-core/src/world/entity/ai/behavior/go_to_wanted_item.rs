//! `GoToWantedItem`.

use super::OneShot;
use super::declarative::BehaviorBuilder;
use super::position_tracker::{EntityTracker, PositionTracker};
use crate::world::entity::ai::memory::WalkTarget;
use crate::world::entity::ai::memory::memory_module_type::{
    ITEM_PICKUP_COOLDOWN_TICKS, LOOK_TARGET, NEAREST_VISIBLE_WANTED_ITEM, WALK_TARGET,
};
use crate::world::entity::ai::sensing::closer_than;
use crate::world::entity::entity::Entity;

/// `create(speedModifier, interruptOngoingWalk, maxDistToWalk)`.
pub fn create(speed_modifier: f32, interrupt_ongoing_walk: bool, max_dist_to_walk: i32) -> OneShot {
    create_with(
        |_| true,
        speed_modifier,
        interrupt_ongoing_walk,
        max_dist_to_walk,
    )
}

/// `create(predicate, speedModifier, interruptOngoingWalk, maxDistToWalk)`.
pub fn create_with(
    predicate: impl Fn(&dyn Entity) -> bool + 'static,
    speed_modifier: f32,
    interrupt_ongoing_walk: bool,
    max_dist_to_walk: i32,
) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let look_target = i.registered(&LOOK_TARGET);
    // the walk condition is `registered` or `absent`; either way only the memory type is needed to set it
    if interrupt_ongoing_walk {
        i.registered(&WALK_TARGET);
    } else {
        i.absent(&WALK_TARGET);
    }
    let wanted_item = i.present(&NEAREST_VISIBLE_WANTED_ITEM);
    let cooldown = i.registered(&ITEM_PICKUP_COOLDOWN_TICKS);
    i.create(move |c| {
        let Some(item) = c.level.get_entity(c.get(&wanted_item).0) else {
            return false;
        };
        if c.try_get(&cooldown).is_none()
            && predicate(&*c.body)
            && closer_than(item, &*c.body, f64::from(max_dist_to_walk))
            && c.level
                .world_border()
                .is_within_bounds_block(&item.block_position())
            && c.body.can_pick_up_loot()
        {
            let target = WalkTarget::new(
                PositionTracker::Entity(EntityTracker::new(item, false)),
                speed_modifier,
                0,
            );
            let look = EntityTracker::new(item, true);
            c.set(&look_target, PositionTracker::Entity(look));
            c.body_mut().brain_mut().set_memory(&WALK_TARGET, target);
            true
        } else {
            false
        }
    })
}
