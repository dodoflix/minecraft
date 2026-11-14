//! The parts of `net.minecraft.world.entity.monster.Ghast` that `HappyGhast` shares: `faceMovementDirection`,
//! `GhastMoveControl` and `RandomFloatAroundGoal`.

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::control::move_control::{MoveControl, MoveControlBase, Operation};
use crate::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use crate::world::entity::inside_block_effects::{
    collided_with_fluid, collided_with_shape_moving_from,
};
use crate::world::entity::mob;
use crate::world::level::block::BlockStateExt as _;
use crate::world::level::collision::for_each_block_intersected_between;
use crate::world::level::material::FluidStateExt as _;
use crate::world::level::{BlockGetter, LevelReader};
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::Attribute;
use mc_world::heightmap::HeightmapType;
use std::sync::Arc;

/// `Ghast.faceMovementDirection(ghast)`.
pub fn face_movement_direction(ghast: &mut dyn Entity, level: &ServerLevel) {
    let pi = std::f32::consts::PI;
    match mob::tick::get_target_checked(ghast, level).and_then(|t| level.get_entity(t)) {
        None => {
            let movement = ghast.base().delta_movement();
            ghast
                .base_mut()
                .set_y_rot(-(mth::atan2(movement.x, movement.z) as f32) * (180.0 / pi));
            let y_rot = ghast.base().y_rot();
            ghast.living_mut().expect("living").y_body_rot = y_rot;
        }
        Some(target) => {
            if target.base().distance_to_sqr_vec(ghast.position()) < 4096.0 {
                let xdd = target.base().x() - ghast.base().x();
                let zdd = target.base().z() - ghast.base().z();
                ghast
                    .base_mut()
                    .set_y_rot(-(mth::atan2(xdd, zdd) as f32) * (180.0 / pi));
                let y_rot = ghast.base().y_rot();
                ghast.living_mut().expect("living").y_body_rot = y_rot;
            }
        }
    }
}

pub type ShouldBeStopped = Arc<dyn Fn(&dyn Entity) -> bool + Send + Sync>;

/// `Ghast.GhastMoveControl`; `should_be_stopped` is the `BooleanSupplier` (reads the controlled mob).
pub struct GhastMoveControl {
    base: MoveControlBase,
    float_duration: i32,
    careful: bool,
    should_be_stopped: ShouldBeStopped,
}

impl GhastMoveControl {
    pub fn new(careful: bool, should_be_stopped: ShouldBeStopped) -> Self {
        Self {
            base: MoveControlBase::default(),
            float_duration: 0,
            careful,
            should_be_stopped,
        }
    }

    /// `canReach(travel)`.
    fn can_reach(&self, mob: &dyn Entity, level: &ServerLevel, travel: Vec3) -> bool {
        let aabb = mob.base().bounding_box();
        let aabb_at_destination = aabb.move_vec(travel);
        if self.careful {
            for pos in BlockPos::between_closed_aabb(&aabb_at_destination.inflate_scalar(1.0)) {
                if !self.block_traversal_possible(mob, level, None, None, pos, false, false) {
                    return false;
                }
            }
        }
        let is_in_water = mob.is_in_water();
        let is_in_lava = mob.is_in_lava();
        let start = mob.position();
        let end = start.add(travel);
        for_each_block_intersected_between(
            start,
            end,
            &aabb_at_destination,
            &mut |block_pos: BlockPos, _i: i32| {
                if aabb.intersects_block_pos(&block_pos) {
                    true
                } else {
                    self.block_traversal_possible(
                        mob,
                        level,
                        Some(start),
                        Some(end),
                        block_pos,
                        is_in_water,
                        is_in_lava,
                    )
                }
            },
        )
    }

    fn block_traversal_possible(
        &self,
        mob: &dyn Entity,
        level: &ServerLevel,
        start: Option<Vec3>,
        end: Option<Vec3>,
        pos: BlockPos,
        can_path_through_water: bool,
        can_path_through_lava: bool,
    ) -> bool {
        let state = level.get_block_state(pos);
        if state.is_air() {
            return true;
        }
        let precise_block_collisions = start.is_some() && end.is_some();
        let path_no_collisions = if let (Some(start), Some(end)) = (start, end) {
            !collided_with_shape_moving_from(
                mob,
                start,
                end,
                &state
                    .get_collision_shape(level, pos)
                    .move_vec(Vec3::new(
                        f64::from(pos.x()),
                        f64::from(pos.y()),
                        f64::from(pos.z()),
                    ))
                    .to_aabbs(),
            )
        } else {
            state.get_collision_shape(level, pos).is_empty()
        };
        if !self.careful {
            return path_no_collisions;
        }
        if state.is_tag("happy_ghast_avoids") {
            return false;
        }
        let fluid_state = level.get_fluid_state(pos);
        if !fluid_state.is_empty()
            && (!precise_block_collisions
                || collided_with_fluid(
                    mob,
                    level,
                    fluid_state,
                    pos,
                    start.expect("start"),
                    end.expect("end"),
                ))
        {
            if fluid_state.is_tag("water") {
                return can_path_through_water;
            }
            if fluid_state.is_tag("lava") {
                return can_path_through_lava;
            }
        }
        path_no_collisions
    }
}

impl MoveControl for GhastMoveControl {
    fn base(&self) -> &MoveControlBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut MoveControlBase {
        &mut self.base
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if (self.should_be_stopped)(mob) {
            self.base.operation = Operation::Wait;
            mob::tick::stop_in_place(mob, level);
        }
        if self.base.operation == Operation::MoveTo {
            let previous = self.float_duration;
            self.float_duration -= 1;
            if previous <= 0 {
                self.float_duration =
                    self.float_duration + mob.base_mut().random.next_int_bounded(5) + 2;
                let travel = Vec3::new(
                    self.base.wanted_x - mob.base().x(),
                    self.base.wanted_y - mob.base().y(),
                    self.base.wanted_z - mob.base().z(),
                );
                if self.can_reach(mob, level, travel) {
                    let flying_speed = mob::attribute_value(mob, Attribute::FLYING_SPEED);
                    let movement = mob.base().delta_movement();
                    mob.base_mut().set_delta_movement(
                        movement.add(travel.normalize().scale(flying_speed * 5.0 / 3.0)),
                    );
                } else {
                    self.base.operation = Operation::Wait;
                }
            }
        }
    }
}

/// `Ghast.RandomFloatAroundGoal`.
pub struct RandomFloatAroundGoal {
    distance_to_blocks: i32,
}

impl RandomFloatAroundGoal {
    const MAX_ATTEMPTS: i32 = 64;

    /// `RandomFloatAroundGoal(ghast, distanceToBlocks)`.
    pub fn new(distance_to_blocks: i32) -> Self {
        Self { distance_to_blocks }
    }

    /// `getSuitableFlyToPosition(mob, distanceToBlocks)`.
    pub fn get_suitable_fly_to_position(
        mob: &mut dyn Entity,
        level: &ServerLevel,
        distance_to_blocks: i32,
    ) -> Vec3 {
        let center = mob.position();
        let mut result: Option<Vec3> = None;
        for _ in 0..Self::MAX_ATTEMPTS {
            result = Self::choose_random_position_with_restriction(mob, center);
            if let Some(r) = result
                && Self::is_good_target(level, r, distance_to_blocks)
            {
                return r;
            }
        }
        let mut result = result.unwrap_or_else(|| Self::choose_random_position(mob, center));
        let pos = BlockPos::containing(result.x, result.y, result.z);
        let height_y = level.get_height_at(HeightmapType::MotionBlocking, pos.x(), pos.z());
        if height_y < pos.y() && height_y > level.get_min_y() {
            result = Vec3::new(
                result.x,
                mob.base().y() - (mob.base().y() - result.y).abs(),
                result.z,
            );
        }
        result
    }

    fn is_good_target(level: &ServerLevel, target: Vec3, distance_to_blocks: i32) -> bool {
        if distance_to_blocks <= 0 {
            return true;
        }
        let pos = BlockPos::containing(target.x, target.y, target.z);
        if !level.get_block_state(pos).is_air() {
            return false;
        }
        for dir in Direction::VALUES {
            for i in 1..distance_to_blocks {
                let offset = pos.relative_n(dir, i);
                if !level.get_block_state(offset).is_air() {
                    return true;
                }
            }
        }
        false
    }

    fn choose_random_position(mob: &mut dyn Entity, center: Vec3) -> Vec3 {
        let random = &mut mob.base_mut().random;
        let x_target = center.x + f64::from((random.next_float() * 2.0 - 1.0) * 16.0);
        let y_target = center.y + f64::from((random.next_float() * 2.0 - 1.0) * 16.0);
        let z_target = center.z + f64::from((random.next_float() * 2.0 - 1.0) * 16.0);
        Vec3::new(x_target, y_target, z_target)
    }

    fn choose_random_position_with_restriction(mob: &mut dyn Entity, center: Vec3) -> Option<Vec3> {
        let target = Self::choose_random_position(mob, center);
        let m = mob.mob().expect("mob");
        if m.has_home() && !m.is_within_home_vec(target) {
            None
        } else {
            Some(target)
        }
    }
}

impl Goal for RandomFloatAroundGoal {
    fn name(&self) -> &'static str {
        "Ghast$RandomFloatAroundGoal"
    }

    fn can_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        let control = mob
            .mob()
            .and_then(|m| m.move_control.as_deref())
            .expect("move control");
        if !control.has_wanted() {
            true
        } else {
            let xd = control.get_wanted_x() - mob.base().x();
            let yd = control.get_wanted_y() - mob.base().y();
            let zd = control.get_wanted_z() - mob.base().z();
            let dd = xd * xd + yd * yd + zd * zd;
            !(1.0..=3600.0).contains(&dd)
        }
    }

    fn can_continue_to_use(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        false
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let result = Self::get_suitable_fly_to_position(mob, level, self.distance_to_blocks);
        mob::set_wanted_position(mob, result.x, result.y, result.z, 1.0);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}
