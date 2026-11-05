//! `Slide`.

use super::breeze::Breeze;
use super::breeze_util;
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::behavior::position_tracker::{BlockPosTracker, PositionTracker};
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::world::entity::ai::memory::WalkTarget;
use mc_game_core::world::entity::ai::memory::memory_module_type::{ATTACK_TARGET, BREEZE_JUMP_COOLDOWN, BREEZE_SHOOT, WALK_TARGET};
use mc_game_core::world::entity::ai::util::default_random_pos;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::Pose;

/// `Slide extends Behavior<Breeze>`.
pub struct Slide;

impl Slide {
    pub fn new() -> BehaviorBase<Slide> {
        BehaviorBase::new(entry_condition![ATTACK_TARGET => ValuePresent, WALK_TARGET => ValueAbsent, BREEZE_JUMP_COOLDOWN => ValueAbsent, BREEZE_SHOOT => ValueAbsent], Slide)
    }
}

/// `randomPointInMiddleCircle(breeze, enemy)`.
fn random_point_in_middle_circle(breeze: &mut dyn Entity, enemy_position: Vec3) -> Vec3 {
    let direction = enemy_position.subtract(breeze.position());
    let distance = direction.length() - mth::lerp_f64(breeze.base_mut().random.next_double(), 8.0, 4.0);
    let target = direction.normalize().multiply_xyz(distance, distance, distance);
    breeze.position().add(target)
}

impl Behavior for Slide {
    fn check_extra_start_conditions(&mut self, _level: &mut ServerLevel, breeze: &mut dyn Entity) -> bool {
        breeze.base().on_ground() && !breeze.is_in_water() && breeze.base().get_pose() == Pose::Standing
    }

    fn start(&mut self, level: &mut ServerLevel, breeze: &mut dyn Entity, _timestamp: i64) {
        let Some(enemy_id) = breeze.brain().get_memory(&ATTACK_TARGET).map(|r| r.0) else { return };
        let Some(enemy_position) = level.get_entity(enemy_id).map(|e| e.position()) else { return };
        let is_within_inner_ring = breeze.as_any().downcast_ref::<Breeze>().is_some_and(|b| b.within_inner_circle_range(enemy_position));
        let mut position = None;
        if is_within_inner_ring
            && let Some(position0) = default_random_pos::get_pos_away(breeze, level, 5, 5, enemy_position)
            && breeze_util::has_line_of_sight(level, breeze, position0)
            && level.get_entity(enemy_id).is_some_and(|e| e.base().distance_to_sqr_vec(position0) > e.base().distance_to_sqr_vec(breeze.position()))
        {
            position = Some(position0);
        }
        let position = match position {
            Some(p) => p,
            None => {
                if breeze.base_mut().random.next_boolean() {
                    let enemy = level.get_entity(enemy_id).expect("enemy resolved above");
                    breeze_util::random_point_behind_target(enemy, &mut breeze.base_mut().random)
                } else {
                    random_point_in_middle_circle(breeze, enemy_position)
                }
            }
        };
        breeze
            .brain_mut()
            .set_memory(&WALK_TARGET, WalkTarget::new(PositionTracker::Block(BlockPosTracker::new(BlockPos::containing_position(&position))), 0.6, 1));
    }
}
