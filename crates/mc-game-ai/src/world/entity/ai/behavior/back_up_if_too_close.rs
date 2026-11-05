//! `BackUpIfTooClose`.

use mc_game_core::world::entity::ai::behavior::declarative::BehaviorBuilder;
use mc_game_core::world::entity::ai::behavior::position_tracker::{EntityTracker, PositionTracker};
use mc_game_core::world::entity::ai::behavior::OneShot;
use mc_game_core::world::entity::ai::memory::memory_module_type::{ATTACK_TARGET, LOOK_TARGET, NEAREST_VISIBLE_LIVING_ENTITIES, WALK_TARGET};
use mc_game_core::world::entity::ai::sensing::closer_than;
use mc_game_core::world::entity::mob::navigation;
use mc_core::math::mth;

/// `create(tooCloseDistance, strafeSpeed)`.
pub fn create(too_close_distance: i32, strafe_speed: f32) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let _walk_target = i.absent(&WALK_TARGET);
    let look_target = i.registered(&LOOK_TARGET);
    let attack_target = i.present(&ATTACK_TARGET);
    let nearest_visible = i.present(&NEAREST_VISIBLE_LIVING_ENTITIES);
    i.create(move |c| {
        let target_id = c.get(&attack_target).0;
        let Some(target) = c.level.get_entity(target_id) else { return false };
        if !(closer_than(target, &*c.body, f64::from(too_close_distance)) && c.get(&nearest_visible).contains(&*c.level, &*c.body, target_id)) {
            return false;
        }
        let tracker = EntityTracker::new(target, true);
        c.set(&look_target, PositionTracker::Entity(tracker));
        // `getMoveControl()` redirects to the controlled vehicle
        match navigation::controlled_vehicle(&*c.body, &*c.level).filter(|&v| c.level.get_entity(v).is_some_and(|v| v.mob().is_some())) {
            Some(vehicle) => {
                c.level.with_entity(vehicle, |_, v| {
                    if let Some(m) = v.mob_mut() {
                        m.move_control_mut().strafe(-strafe_speed, 0.0);
                    }
                });
            }
            None => {
                if let Some(m) = c.body.mob_mut() {
                    m.move_control_mut().strafe(-strafe_speed, 0.0);
                }
            }
        }
        let body = &mut *c.body;
        let y_head_rot = body.get_y_head_rot();
        let y_rot = mth::rotate_if_necessary(body.base().y_rot(), y_head_rot, 0.0);
        body.set_y_rot(y_rot);
        true
    })
}
