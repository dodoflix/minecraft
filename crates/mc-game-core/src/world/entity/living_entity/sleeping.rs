//! Sleeping: `startSleeping`, `stopSleeping`, `setPosToBed`, `checkBedExists`, `getBedOrientation` and the
//! `AbstractBedBlock` helpers they call.
//!
//! The `AbstractBedBlock` statics live in `block::impls::abstract_bed_block`; `findStandUpPosition` stays here.

use super::util::{l, lm};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::dismount_helper::find_safe_dismount_location;
use crate::world::level::block::helpers;
use crate::world::level::block::impls::abstract_bed_block;
use crate::world::level::level_access::LevelAccessor;
use crate::world::level::{BlockGetter, LevelReader};
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::enums::Pose;
use mc_data::{EntityType, props};

fn is_bed(state: BlockState) -> bool {
    helpers::is_instance_of(state, "AbstractBedBlock")
}

/// `AbstractBedBlock.getSleepHeight(state, level, pos)`.
fn get_sleep_height(state: BlockState, level: &ServerLevel, pos: BlockPos) -> Option<f64> {
    abstract_bed_block::get_sleep_height(state, level, pos)
}

/// `AbstractBedBlock.getBedOrientation(level, pos)`.
pub fn get_bed_orientation_at(level: &ServerLevel, pos: BlockPos) -> Option<Direction> {
    let state = level.get_block_state(pos);
    is_bed(state).then(|| state.get(props::HORIZONTAL_FACING))
}

/// `getBedOrientation()`.
pub fn get_bed_orientation(entity: &dyn Entity, level: &ServerLevel) -> Option<Direction> {
    l(entity)
        .get_sleeping_pos()
        .and_then(|pos| get_bed_orientation_at(level, pos))
}

/// `setPosToBed(sleepHeight, bedPosition)`.
fn set_pos_to_bed_height(entity: &mut dyn Entity, sleep_height: f64, bed_position: BlockPos) {
    entity.set_pos(
        f64::from(bed_position.x()) + 0.5,
        f64::from(bed_position.y()) + sleep_height + 0.125,
        f64::from(bed_position.z()) + 0.5,
    );
}

/// `setPosToBed(bedPosition)`.
pub fn set_pos_to_bed(
    entity: &mut dyn Entity,
    level: &ServerLevel,
    bed_position: BlockPos,
) -> bool {
    let state = level.get_block_state(bed_position);
    match get_sleep_height(state, level, bed_position) {
        Some(sleep_height) => {
            set_pos_to_bed_height(entity, sleep_height, bed_position);
            true
        }
        None => false,
    }
}

/// `checkBedExists()`.
pub fn check_bed_exists(entity: &dyn Entity, level: &ServerLevel) -> bool {
    l(entity)
        .get_sleeping_pos()
        .is_some_and(|pos| get_sleep_height(level.get_block_state(pos), level, pos).is_some())
}

/// `startSleeping(bedPosition)`.
pub fn start_sleeping(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    bed_position: BlockPos,
) -> bool {
    let state = level.get_block_state(bed_position);
    let Some(sleep_height) = get_sleep_height(state, level, bed_position) else {
        return false;
    };
    if entity.base().is_passenger() {
        level.stop_riding(entity);
    }
    set_pos_to_bed_height(entity, sleep_height, bed_position);
    level.set_block_and_update(bed_position, state.set(props::OCCUPIED, true));
    entity.set_pose(Pose::Sleeping);
    lm(entity).set_sleeping_pos(bed_position);
    entity.base_mut().set_delta_movement(Vec3::ZERO);
    entity.base_mut().needs_sync = true;
    true
}

/// `stopSleeping()`.
pub fn stop_sleeping(entity: &mut dyn Entity, level: &mut ServerLevel) {
    if let Some(bed_position) = l(entity)
        .get_sleeping_pos()
        .filter(|&pos| LevelReader::has_chunk(level, pos.x() >> 4, pos.z() >> 4))
    {
        let state = level.get_block_state(bed_position);
        if is_bed(state) {
            let facing = state.get(props::HORIZONTAL_FACING);
            level.set_block_and_update(bed_position, state.set(props::OCCUPIED, false));
            abstract_bed_block::on_stop_sleeping(level, bed_position);
            let stand_up = find_stand_up_position(
                entity.get_type(),
                level,
                bed_position,
                facing,
                entity.base().y_rot(),
            )
            .unwrap_or_else(|| {
                let above = bed_position.above();
                Vec3::new(
                    f64::from(above.x()) + 0.5,
                    f64::from(above.y()) + 0.1,
                    f64::from(above.z()) + 0.5,
                )
            });
            let look_direction = Vec3::at_bottom_center_of(&bed_position)
                .subtract(stand_up)
                .normalize();
            let yaw = mth::wrap_degrees_f64(
                mth::atan2(look_direction.z, look_direction.x)
                    * f64::from(mc_core::math::mth::RAD_TO_DEG)
                    - 90.0,
            ) as f32;
            entity.set_pos(stand_up.x, stand_up.y, stand_up.z);
            entity.set_y_rot(yaw);
            entity.set_x_rot(0.0);
        }
    }
    let pos = entity.base().position();
    // `setPos(pos)` after the pose change puts the entity back, so the size-change push-out of `setPose` is dropped
    let pending_fudge = entity.base_mut().pending_fudge_from.take();
    entity.set_pose(Pose::Standing);
    entity.base_mut().pending_fudge_from = pending_fudge;
    entity.set_pos(pos.x, pos.y, pos.z);
    lm(entity).clear_sleeping_pos();
}

fn step(direction: Direction) -> (i32, i32) {
    let s = direction.get_step();
    (s.x(), s.z())
}

fn bed_surround_stand_up_offsets(forward: Direction, side: Direction) -> Vec<(i32, i32)> {
    let ((sx, sz), (fx, fz)) = (step(side), step(forward));
    vec![
        (sx, sz),
        (sx - fx, sz - fz),
        (sx - fx * 2, sz - fz * 2),
        (-fx * 2, -fz * 2),
        (-sx - fx * 2, -sz - fz * 2),
        (-sx - fx, -sz - fz),
        (-sx, -sz),
        (-sx + fx, -sz + fz),
        (fx, fz),
        (sx + fx, sz + fz),
    ]
}

fn bed_above_stand_up_offsets(forward: Direction) -> Vec<(i32, i32)> {
    let (fx, fz) = step(forward);
    vec![(0, 0), (-fx, -fz)]
}

fn find_stand_up_position_at_offset(
    entity_type: EntityType,
    level: &ServerLevel,
    pos: BlockPos,
    offsets: &[(i32, i32)],
    check_dangerous: bool,
) -> Option<Vec3> {
    offsets.iter().find_map(|&(dx, dz)| {
        find_safe_dismount_location(
            entity_type,
            level,
            BlockPos::new(pos.x() + dx, pos.y(), pos.z() + dz),
            check_dangerous,
        )
    })
}

/// `AbstractBedBlock.findStandUpPosition(type, level, pos, forward, yaw)`.
pub fn find_stand_up_position(
    entity_type: EntityType,
    level: &ServerLevel,
    pos: BlockPos,
    forward: Direction,
    yaw: f32,
) -> Option<Vec3> {
    let right = forward.get_clockwise();
    let side = if right.is_facing_angle(yaw) {
        right.get_opposite()
    } else {
        right
    };
    let is_bunk_bed =
        helpers::is_instance_of(level.get_block_state(pos.below()), "AbstractBedBlock");
    if is_bunk_bed {
        let offsets = bed_surround_stand_up_offsets(forward, side);
        if let Some(p) = find_stand_up_position_at_offset(entity_type, level, pos, &offsets, true) {
            return Some(p);
        }
        let below = pos.below();
        if let Some(p) = find_stand_up_position_at_offset(entity_type, level, below, &offsets, true)
        {
            return Some(p);
        }
        let above_offsets = bed_above_stand_up_offsets(forward);
        if let Some(p) =
            find_stand_up_position_at_offset(entity_type, level, pos, &above_offsets, true)
        {
            return Some(p);
        }
        if let Some(p) = find_stand_up_position_at_offset(entity_type, level, pos, &offsets, false)
        {
            return Some(p);
        }
        if let Some(p) =
            find_stand_up_position_at_offset(entity_type, level, below, &offsets, false)
        {
            return Some(p);
        }
        find_stand_up_position_at_offset(entity_type, level, pos, &above_offsets, false)
    } else {
        let mut offsets = bed_surround_stand_up_offsets(forward, side);
        offsets.extend(bed_above_stand_up_offsets(forward));
        find_stand_up_position_at_offset(entity_type, level, pos, &offsets, true)
            .or_else(|| find_stand_up_position_at_offset(entity_type, level, pos, &offsets, false))
    }
}
