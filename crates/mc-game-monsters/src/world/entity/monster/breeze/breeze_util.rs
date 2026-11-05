//! `BreezeUtil`.

use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::movement::collision_context;
use mc_game_core::world::level::collision::{BlockGetterExt, ClipBlock, ClipContext, ClipFluid};
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_data::Attribute;

const MAX_LINE_OF_SIGHT_TEST_RANGE: f64 = 50.0;

/// `randomPointBehindTarget(enemy, random)`.
pub fn random_point_behind_target(enemy: &dyn Entity, random: &mut dyn RandomSource) -> Vec3 {
    let view_angle = enemy.living().map_or(0.0, |l| l.y_head_rot) + 180.0 + random.next_gaussian() as f32 * 90.0 / 2.0;
    let r = mth::lerp_f32(random.next_float(), 4.0, 8.0);
    let direction = Vec3::direction_from_rotation(0.0, view_angle).scale(f64::from(r));
    enemy.position().add(direction)
}

/// `hasLineOfSight(breeze, target)`.
pub fn has_line_of_sight(level: &ServerLevel, breeze: &dyn Entity, target: Vec3) -> bool {
    let from = breeze.position();
    if target.distance_to(from) > get_max_line_of_sight_test_range(breeze) {
        false
    } else {
        level.clip(&ClipContext::new(from, target, ClipBlock::Collider, ClipFluid::None, collision_context(breeze))).miss
    }
}

fn get_max_line_of_sight_test_range(breeze: &dyn Entity) -> f64 {
    MAX_LINE_OF_SIGHT_TEST_RANGE.max(breeze.living().map_or(0.0, |l| l.get_attribute_value(Attribute::FOLLOW_RANGE)))
}
