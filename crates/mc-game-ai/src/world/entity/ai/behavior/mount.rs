//! `Mount`.

use mc_game_core::world::entity::ai::behavior::BehaviorBuilder;
use mc_game_core::world::entity::ai::behavior::OneShot;
use mc_game_core::world::entity::ai::behavior::position_tracker::{EntityTracker, PositionTracker};
use mc_game_core::world::entity::ai::memory::WalkTarget;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    LOOK_TARGET, RIDE_TARGET, WALK_TARGET,
};
use mc_game_core::world::entity::ai::sensing::closer_than;

/// `CLOSE_ENOUGH_TO_START_RIDING_DIST`.
const CLOSE_ENOUGH_TO_START_RIDING_DIST: i32 = 1;

/// `Mount.create(speedModifier)`.
pub fn create(speed_modifier: f32) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let look_target = i.registered(&LOOK_TARGET);
    let walk_target = i.absent(&WALK_TARGET);
    let ride_target = i.present(&RIDE_TARGET);
    i.create(move |c| {
        if c.body.base().is_passenger() {
            return false;
        }
        let ridable_id = c.get(&ride_target).0;
        let Some(ridable) = c.level.get_entity(ridable_id) else {
            return false;
        };
        if closer_than(
            ridable,
            &*c.body,
            f64::from(CLOSE_ENOUGH_TO_START_RIDING_DIST),
        ) {
            let (level, body) = (&mut *c.level, &mut *c.body);
            level.start_riding(body, ridable_id, false, true);
        } else {
            let look = PositionTracker::Entity(EntityTracker::new(ridable, true));
            let walk = WalkTarget::new(
                PositionTracker::Entity(EntityTracker::new(ridable, false)),
                speed_modifier,
                CLOSE_ENOUGH_TO_START_RIDING_DIST,
            );
            c.set(&look_target, look);
            c.set(&walk_target, walk);
        }
        true
    })
}
