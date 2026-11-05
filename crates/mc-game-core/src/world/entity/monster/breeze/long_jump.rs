//! `LongJump` (the breeze's).

use super::breeze_util;
use crate::entry_condition;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::behavior::long_jump_util;
use crate::world::entity::ai::behavior::{Behavior, BehaviorBase};
use crate::world::entity::ai::memory::memory_module_type::{
    ATTACK_TARGET, BREEZE_JUMP_COOLDOWN, BREEZE_JUMP_INHALING, BREEZE_JUMP_TARGET, BREEZE_LEAVING_WATER, BREEZE_SHOOT, HURT_BY, WALK_TARGET,
};
use crate::world::entity::ai::memory::{MemoryStatus, Unit};
use crate::world::entity::dismount_helper::is_block_dangerous;
use crate::world::entity::living_entity::jump::get_jump_boost_power;
use crate::world::level::collision::{BlockGetterExt, ClipBlock, ClipContext, ClipFluid};
use crate::world::level::BlockGetter;
use crate::world::level::material::FluidStateExt as _;
use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_data::enums::Pose;
use mc_data::{Attribute, MobEffect, SoundEvent};
use mc_protocol::packets::play::SoundSource;

const REQUIRED_AIR_BLOCKS_ABOVE: i32 = 4;
const JUMP_COOLDOWN_TICKS: i64 = 10;
const JUMP_COOLDOWN_WHEN_HURT_TICKS: i64 = 2;
/// `INHALING_DURATION_TICKS = Math.round(10.0F)`.
const INHALING_DURATION_TICKS: i64 = 10;
const MAX_JUMP_VELOCITY_MULTIPLIER: f32 = 0.058333334;
const ALLOWED_ANGLES: [i32; 5] = [40, 55, 60, 75, 80];

/// `LongJump extends Behavior<Breeze>`.
pub struct LongJump;

impl LongJump {
    pub fn new() -> BehaviorBase<LongJump> {
        BehaviorBase::with_timeout(
            entry_condition![
                ATTACK_TARGET => ValuePresent,
                BREEZE_JUMP_COOLDOWN => ValueAbsent,
                BREEZE_JUMP_INHALING => Registered,
                BREEZE_JUMP_TARGET => Registered,
                BREEZE_SHOOT => ValueAbsent,
                WALK_TARGET => ValueAbsent,
                BREEZE_LEAVING_WATER => Registered
            ],
            200,
            LongJump,
        )
    }

    /// `canRun(level, breeze)`.
    pub fn can_run(level: &mut ServerLevel, breeze: &mut dyn Entity) -> bool {
        if !breeze.base().on_ground() && !breeze.is_in_water() {
            return false;
        }
        if breeze.get_fluid_height("entity_floatable") > breeze.get_fluid_jump_threshold() || breeze.is_in_lava() {
            return false;
        }
        if breeze.brain().check_memory(BREEZE_JUMP_TARGET.id(), MemoryStatus::ValuePresent) {
            return true;
        }
        let Some(attack_target) = breeze.brain().get_memory(&ATTACK_TARGET).map(|r| r.0) else { return false };
        let Some(target_position) = level.get_entity(attack_target).map(|t| t.position()) else { return false };
        if out_of_aggro_range(level, breeze, attack_target) {
            breeze.brain_mut().erase_memory(&ATTACK_TARGET);
            return false;
        }
        if too_close_for_jump(level, breeze, attack_target) || !can_jump_from_current_position(level, breeze) {
            return false;
        }
        let _ = target_position;
        let behind = {
            let target = level.get_entity(attack_target).expect("resolved above");
            breeze_util::random_point_behind_target(target, &mut breeze.base_mut().random)
        };
        let Some(target_pos) = snap_to_surface(level, breeze, behind) else { return false };
        let below = level.get_block_state(target_pos.below());
        if is_block_dangerous(breeze.get_type(), below) {
            return false;
        }
        if !breeze_util::has_line_of_sight(level, breeze, Vec3::at_center_of(&target_pos)) && !breeze_util::has_line_of_sight(level, breeze, Vec3::at_center_of(&target_pos.above_n(4))) {
            return false;
        }
        breeze.brain_mut().set_memory(&BREEZE_JUMP_TARGET, target_pos);
        true
    }
}

fn is_finished_inhaling(breeze: &dyn Entity) -> bool {
    breeze.brain().get_memory(&BREEZE_JUMP_INHALING).is_none() && breeze.base().get_pose() == Pose::Inhaling
}

fn is_finished_jumping(breeze: &dyn Entity) -> bool {
    let is_jumping = breeze.base().get_pose() == Pose::LongJumping;
    let landed_on_ground = breeze.base().on_ground();
    let landed_in_water = breeze.is_in_water() && breeze.brain().check_memory(BREEZE_LEAVING_WATER.id(), MemoryStatus::ValueAbsent);
    is_jumping && (landed_on_ground || landed_in_water)
}

/// `snapToSurface(entity, target)`.
fn snap_to_surface(level: &ServerLevel, entity: &dyn Entity, target: Vec3) -> Option<BlockPos> {
    let ctx = crate::world::entity::movement::collision_context(entity);
    let below = level.clip(&ClipContext::new(target, target.relative(Direction::Down, 10.0), ClipBlock::Collider, ClipFluid::None, ctx.clone()));
    if !below.miss {
        return Some(BlockPos::containing_position(&below.location).above());
    }
    let above = level.clip(&ClipContext::new(target, target.relative(Direction::Up, 10.0), ClipBlock::Collider, ClipFluid::None, ctx));
    if !above.miss { Some(BlockPos::containing_position(&above.location).above()) } else { None }
}

fn out_of_aggro_range(level: &ServerLevel, breeze: &dyn Entity, attack_target: crate::world::entity::EntityId) -> bool {
    let range = breeze.living().map_or(0.0, |l| l.get_attribute_value(Attribute::FOLLOW_RANGE));
    !level.get_entity(attack_target).is_some_and(|t| crate::world::entity::ai::sensing::closer_than(t, breeze, range))
}

fn too_close_for_jump(level: &ServerLevel, breeze: &dyn Entity, attack_target: crate::world::entity::EntityId) -> bool {
    level.get_entity(attack_target).is_some_and(|t| t.base().distance_to_sqr_vec(breeze.position()).sqrt() as f32 - 4.0 <= 0.0)
}

fn can_jump_from_current_position(level: &ServerLevel, breeze: &dyn Entity) -> bool {
    let current_pos = breeze.base().block_position();
    if level.get_block_state(current_pos).block() == Block::HONEY_BLOCK {
        return false;
    }
    for i in 1..=REQUIRED_AIR_BLOCKS_ABOVE {
        let offset_pos = current_pos.above_n(i);
        if !level.get_block_state(offset_pos).is_air() && !level.get_fluid_state(offset_pos).is_tag("water") {
            return false;
        }
    }
    true
}

/// `calculateOptimalJumpVector(body, random, targetPos)`.
fn calculate_optimal_jump_vector(level: &ServerLevel, body: &mut dyn Entity, target_pos: Vec3) -> Option<Vec3> {
    for angle in mc_core::direction::shuffled_copy(&ALLOWED_ANGLES, &mut body.base_mut().random) {
        let max_jump_velocity = MAX_JUMP_VELOCITY_MULTIPLIER * body.living().map_or(0.0, |l| l.get_attribute_value(Attribute::FOLLOW_RANGE)) as f32;
        if let Some(velocity) = long_jump_util::calculate_jump_vector_for_angle(level, body, target_pos, max_jump_velocity, angle, false) {
            if body.has_effect(MobEffect::JUMP_BOOST) {
                let jump_effect_amplifier = velocity.normalize().y * f64::from(get_jump_boost_power(body));
                return Some(velocity.add_xyz(0.0, jump_effect_amplifier, 0.0));
            }
            return Some(velocity);
        }
    }
    None
}

impl Behavior for LongJump {
    fn check_extra_start_conditions(&mut self, level: &mut ServerLevel, breeze: &mut dyn Entity) -> bool {
        Self::can_run(level, breeze)
    }

    fn can_still_use(&mut self, _level: &mut ServerLevel, breeze: &mut dyn Entity, _timestamp: i64) -> bool {
        breeze.base().get_pose() != Pose::Standing && !breeze.brain().has_memory_value(BREEZE_JUMP_COOLDOWN)
    }

    fn start(&mut self, level: &mut ServerLevel, breeze: &mut dyn Entity, _timestamp: i64) {
        if breeze.brain().check_memory(BREEZE_JUMP_INHALING.id(), MemoryStatus::ValueAbsent) {
            breeze.brain_mut().set_memory_with_expiry(&BREEZE_JUMP_INHALING, Unit, INHALING_DURATION_TICKS);
        }
        breeze.set_pose(Pose::Inhaling);
        let p = breeze.position();
        level.play_sound_from_entity_at(None, breeze.id(), p, SoundEvent::ENTITY_BREEZE_CHARGE, SoundSource::Hostile, 1.0, 1.0);
        if let Some(target_pos) = breeze.brain().get_memory(&BREEZE_JUMP_TARGET).copied() {
            breeze.look_at(crate::commands::arguments::entity_anchor_argument::Anchor::Eyes, Vec3::at_center_of(&target_pos));
        }
    }

    fn tick(&mut self, level: &mut ServerLevel, breeze: &mut dyn Entity, _timestamp: i64) {
        let in_water = breeze.is_in_water();
        if !in_water && breeze.brain().check_memory(BREEZE_LEAVING_WATER.id(), MemoryStatus::ValuePresent) {
            breeze.brain_mut().erase_memory(&BREEZE_LEAVING_WATER);
        }
        if is_finished_inhaling(breeze) {
            let velocity_vector = breeze.brain().get_memory(&BREEZE_JUMP_TARGET).copied().and_then(|target_pos| calculate_optimal_jump_vector(level, breeze, Vec3::at_bottom_center_of(&target_pos)));
            let Some(velocity_vector) = velocity_vector else {
                breeze.set_pose(Pose::Standing);
                return;
            };
            if in_water {
                breeze.brain_mut().set_memory(&BREEZE_LEAVING_WATER, Unit);
            }
            breeze.play_sound(level, SoundEvent::ENTITY_BREEZE_JUMP, 1.0, 1.0);
            breeze.set_pose(Pose::LongJumping);
            let y_body_rot = breeze.living().map_or(0.0, |l| l.y_body_rot);
            breeze.set_y_rot(y_body_rot);
            if let Some(l) = breeze.living_mut() {
                l.discard_friction = true;
            }
            breeze.base_mut().set_delta_movement(velocity_vector);
        } else if is_finished_jumping(breeze) {
            breeze.play_sound(level, SoundEvent::ENTITY_BREEZE_LAND, 1.0, 1.0);
            breeze.set_pose(Pose::Standing);
            if let Some(l) = breeze.living_mut() {
                l.discard_friction = false;
            }
            let was_hurt = breeze.brain().has_memory_value(HURT_BY);
            breeze.brain_mut().set_memory_with_expiry(&BREEZE_JUMP_COOLDOWN, Unit, if was_hurt { JUMP_COOLDOWN_WHEN_HURT_TICKS } else { JUMP_COOLDOWN_TICKS });
            breeze.brain_mut().set_memory_with_expiry(&BREEZE_SHOOT, Unit, 100);
        }
    }

    fn stop(&mut self, _level: &mut ServerLevel, breeze: &mut dyn Entity, _timestamp: i64) {
        if matches!(breeze.base().get_pose(), Pose::LongJumping | Pose::Inhaling) {
            breeze.set_pose(Pose::Standing);
        }
        breeze.brain_mut().erase_memory(&BREEZE_JUMP_TARGET);
        breeze.brain_mut().erase_memory(&BREEZE_JUMP_INHALING);
        breeze.brain_mut().erase_memory(&BREEZE_LEAVING_WATER);
    }
}

