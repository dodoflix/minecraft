//! `LivingEntity.tick()`: using-item tick, swim amount, arrow/stinger decay, equipment detection, body/head turn.

use super::util::{l, lm};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::living::{DATA_ARROW_COUNT_ID, DATA_STINGER_COUNT_ID};
use mc_core::math::mth;
use mc_data::enums::Pose;

/// `LivingEntity.isVisuallySwimming()`.
pub fn is_visually_swimming(entity: &dyn Entity) -> bool {
    let pose = entity.base().get_pose();
    pose == Pose::Swimming || (!l(entity).is_fall_flying() && pose == Pose::FallFlying)
}

/// `updateSwimAmount()`.
fn update_swim_amount(entity: &mut dyn Entity) {
    let visually_swimming = is_visually_swimming(entity);
    let living = lm(entity);
    living.swim_amount_o = living.swim_amount;
    if visually_swimming {
        living.swim_amount = (living.swim_amount + 0.09).min(1.0);
    } else {
        living.swim_amount = (living.swim_amount - 0.09).max(0.0);
    }
}

/// `LivingEntity.tick()`.
pub fn tick(entity: &mut dyn Entity, level: &mut ServerLevel) {
    // `super.tick()` = `Entity.tick()` = `baseTick()`
    entity.base_tick(level);
    entity.updating_using_item(level);
    update_swim_amount(entity);
    // server side
    let arrow_count = entity.base().entity_data.get(DATA_ARROW_COUNT_ID);
    if arrow_count > 0 {
        let living = lm(entity);
        if living.remove_arrow_time <= 0 {
            living.remove_arrow_time = 20 * (30 - arrow_count);
        }
        living.remove_arrow_time -= 1;
        if living.remove_arrow_time <= 0 {
            entity.base_mut().entity_data.set(DATA_ARROW_COUNT_ID, arrow_count - 1);
        }
    }
    let stinger_count = entity.base().entity_data.get(DATA_STINGER_COUNT_ID);
    if stinger_count > 0 {
        let living = lm(entity);
        if living.remove_stinger_time <= 0 {
            living.remove_stinger_time = 20 * (30 - stinger_count);
        }
        living.remove_stinger_time -= 1;
        if living.remove_stinger_time <= 0 {
            entity.base_mut().entity_data.set(DATA_STINGER_COUNT_ID, stinger_count - 1);
        }
    }
    super::equipment_changes::detect_equipment_updates(entity, level);
    if entity.base().tick_count % 20 == 0 {
        super::combat::recheck_status(entity, level);
    }
    if entity.is_sleeping() && (!entity.can_interact_with_level() || !super::sleeping::check_bed_exists(entity, level)) {
        super::sleeping::stop_sleeping(entity, level);
    }
    if !entity.is_removed() {
        entity.ai_step(level);
    }
    let xd = entity.base().x() - entity.base().xo;
    let zd = entity.base().z() - entity.base().zo;
    let side_dist = (xd * xd + zd * zd) as f32;
    let mut y_body_rot_t = l(entity).y_body_rot;
    if side_dist > 0.0025000002 {
        let walk_direction = mth::atan2(zd, xd) as f32 * (180.0 / std::f64::consts::PI as f32) - 90.0;
        let diff_between_direction_and_facing = mth::abs_f32(mth::wrap_degrees_f32(entity.base().y_rot()) - walk_direction);
        y_body_rot_t = if 95.0 < diff_between_direction_and_facing && diff_between_direction_and_facing < 265.0 { walk_direction - 180.0 } else { walk_direction };
    }
    if l(entity).swing_state.animation() > 0.0 {
        y_body_rot_t = entity.base().y_rot();
    }
    entity.tick_head_turn(y_body_rot_t);
    // range checks
    let b = entity.base_mut();
    while b.y_rot() - b.y_rot_o < -180.0 {
        b.y_rot_o -= 360.0;
    }
    while b.y_rot() - b.y_rot_o >= 180.0 {
        b.y_rot_o += 360.0;
    }
    while b.x_rot() - b.x_rot_o < -180.0 {
        b.x_rot_o -= 360.0;
    }
    while b.x_rot() - b.x_rot_o >= 180.0 {
        b.x_rot_o += 360.0;
    }
    let (y_rot, x_rot) = (entity.base().y_rot(), entity.base().x_rot());
    let _ = (y_rot, x_rot);
    let living = lm(entity);
    while living.y_body_rot - living.y_body_rot_o < -180.0 {
        living.y_body_rot_o -= 360.0;
    }
    while living.y_body_rot - living.y_body_rot_o >= 180.0 {
        living.y_body_rot_o += 360.0;
    }
    while living.y_head_rot - living.y_head_rot_o < -180.0 {
        living.y_head_rot_o -= 360.0;
    }
    while living.y_head_rot - living.y_head_rot_o >= 180.0 {
        living.y_head_rot_o += 360.0;
    }
    if l(entity).is_fall_flying() {
        lm(entity).fall_fly_ticks += 1;
    } else {
        lm(entity).fall_fly_ticks = 0;
    }
    if entity.is_sleeping() {
        entity.set_x_rot(0.0);
    }
    super::effects::refresh_dirty_attributes(entity, level);
    if l(entity).current_impulse_context_reset_grace_time > 0 {
        lm(entity).current_impulse_context_reset_grace_time -= 1;
    }
}

/// `tickHeadTurn(yBodyRotT)`.
pub fn tick_head_turn(entity: &mut dyn Entity, y_body_rot_t: f32) {
    let y_body_rot_d = mth::wrap_degrees_f32(y_body_rot_t - l(entity).y_body_rot);
    lm(entity).y_body_rot += y_body_rot_d * 0.3;
    let head_diff = mth::wrap_degrees_f32(entity.base().y_rot() - l(entity).y_body_rot);
    let max_head_rotation = entity.get_max_head_rotation_relative_to_body();
    if head_diff.abs() > max_head_rotation {
        lm(entity).y_body_rot += head_diff - mth::sign(f64::from(head_diff)) as f32 * max_head_rotation;
    }
}
