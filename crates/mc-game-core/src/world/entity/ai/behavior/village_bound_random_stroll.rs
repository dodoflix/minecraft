//! `VillageBoundRandomStroll`.

use super::{BehaviorBuilder, OneShot};
use crate::world::entity::ai::memory::WalkTarget;
use crate::world::entity::ai::memory::memory_module_type::WALK_TARGET;
use crate::world::entity::ai::util::{default_random_pos, land_random_pos};
use mc_core::phys::Vec3;
use mc_core::pos::SectionPos;

const MAX_XZ_DIST: i32 = 10;
const MAX_Y_DIST: i32 = 7;

/// `create(speedModifier)`.
pub fn create(speed_modifier: f32) -> OneShot {
    create_with(speed_modifier, MAX_XZ_DIST, MAX_Y_DIST)
}

/// `create(speedModifier, maxXyDist, maxYDist)`.
pub fn create_with(speed_modifier: f32, max_xy_dist: i32, max_y_dist: i32) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let walk_target = i.absent(&WALK_TARGET);
    i.create(move |c| {
        let body_pos = c.body().block_position();
        let land_pos = if c.level.is_village(body_pos) {
            land_random_pos::get_pos(c.body, c.level, max_xy_dist, max_y_dist)
        } else {
            let section_pos = SectionPos::of_block_pos(&body_pos);
            let optimal_section_pos = crate::world::entity::ai::goal::move_back_to_village::find_section_closest_to_village(c.level, section_pos, 2);
            if optimal_section_pos != section_pos {
                default_random_pos::get_pos_towards(c.body, c.level, max_xy_dist, max_y_dist, Vec3::at_bottom_center_of(&optimal_section_pos.center()), std::f64::consts::FRAC_PI_2)
            } else {
                land_random_pos::get_pos(c.body, c.level, max_xy_dist, max_y_dist)
            }
        };
        c.set_or_erase(&walk_target, land_pos.map(|pos| WalkTarget::from_vec3(pos, speed_modifier, 0)));
        true
    })
}
