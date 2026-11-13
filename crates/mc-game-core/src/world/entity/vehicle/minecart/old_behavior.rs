//! `net.minecraft.world.entity.vehicle.minecart.OldMinecartBehavior` (server side): the rail movement every cart has unless the
//! `minecart_improvements` experiment is enabled.

use super::abstract_minecart::{
    self, AbstractMinecart, current_block_pos_or_rail_below, exits, is_redstone_conductor,
    rail_shape,
};
use crate::server::level::ServerLevel;
use crate::world::entity::living_entity::push::pushable_entities_in;
use crate::world::entity::{MoverType, ServerPlayer};
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use crate::world::level::block::impls::base_rail_block;
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_data::{EntityType, props};

/// `getMaxSpeed(level)`: 0.2 in water, 0.4 otherwise.
pub fn get_max_speed(in_water: bool) -> f64 {
    if in_water { 0.2 } else { 0.4 }
}

/// `getSlowdownFactor()`.
pub fn get_slowdown_factor(is_vehicle: bool) -> f64 {
    if is_vehicle { 0.997 } else { 0.96 }
}

/// `getMotionDirection()`.
pub fn get_motion_direction<M: AbstractMinecart + ?Sized>(m: &M) -> Direction {
    if m.minecart().flipped {
        m.get_direction().get_opposite().get_clockwise()
    } else {
        m.get_direction().get_clockwise()
    }
}

/// `getKnownMovement(knownMovement)`: the movement a client could have reported, at most 0.4 per axis.
pub fn get_known_movement(known_movement: Vec3) -> Vec3 {
    if !known_movement.x.is_nan() && !known_movement.y.is_nan() && !known_movement.z.is_nan() {
        Vec3::new(
            mth::clamp_f64(known_movement.x, -0.4, 0.4),
            known_movement.y,
            mth::clamp_f64(known_movement.z, -0.4, 0.4),
        )
    } else {
        Vec3::ZERO
    }
}

/// `tick()`.
pub fn tick<M: AbstractMinecart + ?Sized>(m: &mut M, level: &mut ServerLevel) {
    m.apply_gravity();
    let pos = current_block_pos_or_rail_below(m.base(), &*level);
    let state = level.get_block_state(pos);
    let on_rails = base_rail_block::is_rail_state(state);
    m.minecart_mut().on_rails = on_rails;
    if on_rails {
        move_along_track(m, level);
        if state.is_block(Block::ACTIVATOR_RAIL) {
            m.activate_minecart(level, pos.x(), pos.y(), pos.z(), state.get(props::POWERED));
        }
    } else {
        abstract_minecart::come_off_track(m, level);
    }
    m.apply_effects_from_blocks(level);
    m.set_x_rot(0.0);
    let x_diff = m.base().xo - m.base().x();
    let z_diff = m.base().zo - m.base().z();
    if x_diff * x_diff + z_diff * z_diff > 0.001 {
        m.set_y_rot((mth::atan2(z_diff, x_diff) * 180.0 / std::f64::consts::PI) as f32);
        if m.minecart().flipped {
            let y_rot = m.base().y_rot() + 180.0;
            m.set_y_rot(y_rot);
        }
    }
    let rot_diff = f64::from(mth::wrap_degrees_f32(m.base().y_rot() - m.base().y_rot_o));
    if !(-170.0..170.0).contains(&rot_diff) {
        let y_rot = m.base().y_rot() + 180.0;
        m.set_y_rot(y_rot);
        let flipped = m.minecart().flipped;
        m.minecart_mut().flipped = !flipped;
    }
    let x_rot = m.base().x_rot() % 360.0;
    m.set_x_rot(x_rot);
    let y_rot = m.base().y_rot() % 360.0;
    m.set_y_rot(y_rot);
    push_and_pickup_entities(m, level);
}

/// `moveAlongTrack(level)`.
pub fn move_along_track<M: AbstractMinecart + ?Sized>(m: &mut M, level: &mut ServerLevel) {
    let pos = current_block_pos_or_rail_below(m.base(), &*level);
    let state = level.get_block_state(pos);
    m.reset_fall_distance();
    let mut x = m.base().x();
    let mut y = m.base().y();
    let mut z = m.base().z();
    let old_pos = get_pos(&*level, x, y, z);
    y = f64::from(pos.y());
    let mut power_track = false;
    let mut halt_track = false;
    if state.is_block(Block::POWERED_RAIL) {
        power_track = state.get(props::POWERED);
        halt_track = !power_track;
    }

    let mut slide_speed = 0.0078125;
    if m.is_in_water() {
        slide_speed *= 0.2;
    }

    let mut movement = m.base().delta_movement();
    let shape = rail_shape(state);
    use mc_data::enums::RailShape;
    match shape {
        RailShape::AscendingEast => {
            m.base_mut()
                .set_delta_movement(movement.add_xyz(-slide_speed, 0.0, 0.0));
            y += 1.0;
        }
        RailShape::AscendingWest => {
            m.base_mut()
                .set_delta_movement(movement.add_xyz(slide_speed, 0.0, 0.0));
            y += 1.0;
        }
        RailShape::AscendingNorth => {
            m.base_mut()
                .set_delta_movement(movement.add_xyz(0.0, 0.0, slide_speed));
            y += 1.0;
        }
        RailShape::AscendingSouth => {
            m.base_mut()
                .set_delta_movement(movement.add_xyz(0.0, 0.0, -slide_speed));
            y += 1.0;
        }
        _ => {}
    }

    movement = m.base().delta_movement();
    let (exit0, exit1) = exits(shape);
    let mut x_d = f64::from(exit1[0] - exit0[0]);
    let mut z_d = f64::from(exit1[2] - exit0[2]);
    let length = (x_d * x_d + z_d * z_d).sqrt();
    let flip = movement.x * x_d + movement.z * z_d;
    if flip < 0.0 {
        x_d = -x_d;
        z_d = -z_d;
    }

    let pow = 2.0f64.min(movement.horizontal_distance());
    movement = Vec3::new(pow * x_d / length, movement.y, pow * z_d / length);
    m.base_mut().set_delta_movement(movement);
    let rider_intent = m
        .base()
        .passengers
        .first()
        .and_then(|&id| level.get_entity(id))
        .and_then(|rider| rider.as_any().downcast_ref::<ServerPlayer>())
        .map_or(Vec3::ZERO, ServerPlayer::get_last_client_move_intent);
    if !m.base().passengers.is_empty() && rider_intent.length_sqr() > 0.0 {
        let own_dist = m.base().delta_movement().horizontal_distance_sqr();
        if own_dist < 0.01 {
            let d = m.base().delta_movement();
            m.base_mut().set_delta_movement(d.add_xyz(
                rider_intent.x * 0.001,
                0.0,
                rider_intent.z * 0.001,
            ));
            halt_track = false;
        }
    }

    if halt_track {
        let d = m.base().delta_movement();
        let speed_length = d.horizontal_distance();
        if speed_length < 0.03 {
            m.base_mut().set_delta_movement(Vec3::ZERO);
        } else {
            m.base_mut()
                .set_delta_movement(d.multiply_xyz(0.5, 0.0, 0.5));
        }
    }

    let x0 = f64::from(pos.x()) + 0.5 + f64::from(exit0[0]) * 0.5;
    let z0 = f64::from(pos.z()) + 0.5 + f64::from(exit0[2]) * 0.5;
    let x1 = f64::from(pos.x()) + 0.5 + f64::from(exit1[0]) * 0.5;
    let z1 = f64::from(pos.z()) + 0.5 + f64::from(exit1[2]) * 0.5;
    x_d = x1 - x0;
    z_d = z1 - z0;
    let progress = if x_d == 0.0 {
        z - f64::from(pos.z())
    } else if z_d == 0.0 {
        x - f64::from(pos.x())
    } else {
        let xx = x - x0;
        let zz = z - z0;
        (xx * x_d + zz * z_d) * 2.0
    };
    x = x0 + x_d * progress;
    z = z0 + z_d * progress;
    m.set_pos(x, y, z);
    let scale = if m.base().is_vehicle() { 0.75 } else { 1.0 };
    let max_speed = m.get_max_speed(level);
    movement = m.base().delta_movement();
    m.r#move(
        level,
        MoverType::SelfMover,
        Vec3::new(
            mth::clamp_f64(scale * movement.x, -max_speed, max_speed),
            0.0,
            mth::clamp_f64(scale * movement.z, -max_speed, max_speed),
        ),
    );
    let (cx, cy, cz) = (m.base().x(), m.base().y(), m.base().z());
    if exit0[1] != 0
        && mth::floor_f64(cx) - pos.x() == exit0[0]
        && mth::floor_f64(cz) - pos.z() == exit0[2]
    {
        m.set_pos(cx, cy + f64::from(exit0[1]), cz);
    } else if exit1[1] != 0
        && mth::floor_f64(cx) - pos.x() == exit1[0]
        && mth::floor_f64(cz) - pos.z() == exit1[2]
    {
        m.set_pos(cx, cy + f64::from(exit1[1]), cz);
    }

    let slowed = m.apply_natural_slowdown(m.base().delta_movement());
    m.base_mut().set_delta_movement(slowed);
    let new_pos = get_pos(&*level, m.base().x(), m.base().y(), m.base().z());
    if let (Some(new_pos), Some(old_pos)) = (new_pos, old_pos) {
        let speed = (old_pos.y - new_pos.y) * 0.05;
        let d = m.base().delta_movement();
        let other_pow = d.horizontal_distance();
        if other_pow > 0.0 {
            m.base_mut().set_delta_movement(d.multiply_xyz(
                (other_pow + speed) / other_pow,
                1.0,
                (other_pow + speed) / other_pow,
            ));
        }
        let (px, pz) = (m.base().x(), m.base().z());
        m.set_pos(px, new_pos.y, pz);
    }

    let xn = mth::floor_f64(m.base().x());
    let zn = mth::floor_f64(m.base().z());
    if xn != pos.x() || zn != pos.z() {
        let d = m.base().delta_movement();
        let other_pow = d.horizontal_distance();
        m.base_mut().set_delta_movement_xyz(
            other_pow * f64::from(xn - pos.x()),
            d.y,
            other_pow * f64::from(zn - pos.z()),
        );
    }

    if power_track {
        let d = m.base().delta_movement();
        let speed_length = d.horizontal_distance();
        if speed_length > 0.01 {
            m.base_mut().set_delta_movement(d.add_xyz(
                d.x / speed_length * 0.06,
                0.0,
                d.z / speed_length * 0.06,
            ));
        } else {
            let mut dx = d.x;
            let mut dz = d.z;
            if shape == RailShape::EastWest {
                if is_redstone_conductor(level, pos.west()) {
                    dx = 0.02;
                } else if is_redstone_conductor(level, pos.east()) {
                    dx = -0.02;
                }
            } else {
                if shape != RailShape::NorthSouth {
                    return;
                }
                if is_redstone_conductor(level, pos.north()) {
                    dz = 0.02;
                } else if is_redstone_conductor(level, pos.south()) {
                    dz = -0.02;
                }
            }
            m.base_mut().set_delta_movement_xyz(dx, d.y, dz);
        }
    }
}

/// `getPos(x, y, z)`: the point of the rail at `(x, z)` (the rail below counts when there is none at `y`), `None` off the rails.
pub fn get_pos(level: &ServerLevel, x: f64, y: f64, z: f64) -> Option<Vec3> {
    let xt = mth::floor_f64(x);
    let mut yt = mth::floor_f64(y);
    let zt = mth::floor_f64(z);
    if level
        .get_block_state(BlockPos::new(xt, yt - 1, zt))
        .is_tag("rails")
    {
        yt -= 1;
    }
    let state = level.get_block_state(BlockPos::new(xt, yt, zt));
    if !base_rail_block::is_rail_state(state) {
        return None;
    }
    let (exit0, exit1) = exits(rail_shape(state));
    let (xt_d, yt_d, zt_d) = (f64::from(xt), f64::from(yt), f64::from(zt));
    let x0 = xt_d + 0.5 + f64::from(exit0[0]) * 0.5;
    let y0 = yt_d + 0.0625 + f64::from(exit0[1]) * 0.5;
    let z0 = zt_d + 0.5 + f64::from(exit0[2]) * 0.5;
    let x1 = xt_d + 0.5 + f64::from(exit1[0]) * 0.5;
    let y1 = yt_d + 0.0625 + f64::from(exit1[1]) * 0.5;
    let z1 = zt_d + 0.5 + f64::from(exit1[2]) * 0.5;
    let x_d = x1 - x0;
    let y_d = (y1 - y0) * 2.0;
    let z_d = z1 - z0;
    let progress = if x_d == 0.0 {
        z - zt_d
    } else if z_d == 0.0 {
        x - xt_d
    } else {
        let xx = x - x0;
        let zz = z - z0;
        (xx * x_d + zz * z_d) * 2.0
    };
    let x = x0 + x_d * progress;
    let mut y = y0 + y_d * progress;
    let z = z0 + z_d * progress;
    if y_d < 0.0 {
        y += 1.0;
    } else if y_d > 0.0 {
        y += 0.5;
    }
    Some(Vec3::new(x, y, z))
}

/// `pushAndPickupEntities()`: a rideable cart that is fast enough picks up the mobs it meets (everything but players, iron
/// golems and carts) when it has no rider yet, otherwise pushes the entities; a slow or non-rideable cart only pushes carts.
pub fn push_and_pickup_entities<M: AbstractMinecart + ?Sized>(m: &mut M, level: &mut ServerLevel) {
    let hitbox = m
        .base()
        .bounding_box()
        .inflate(f64::from(0.2f32), 0.0, f64::from(0.2f32));
    if m.is_rideable() && m.base().delta_movement().horizontal_distance_sqr() >= 0.01 {
        for id in pushable_entities_in(level, m.as_dyn(), &hitbox) {
            level.with_entity(id, |level, other| {
                if !other.is_player()
                    && other.get_type() != EntityType::IRON_GOLEM
                    && other.as_abstract_minecart().is_none()
                    && !m.base().is_vehicle()
                    && !other.base().is_passenger()
                {
                    level.start_riding_held(other, m.as_dyn_mut(), false, true);
                } else {
                    other.push_entity(level, m.as_dyn_mut());
                }
            });
        }
    } else {
        let own = m.base().id;
        for id in level.get_entities(Some(own), &hitbox, |_| true) {
            level.with_entity(id, |level, other| {
                if !m.base().passengers.contains(&id)
                    && other.is_pushable(level)
                    && other.as_abstract_minecart().is_some()
                {
                    other.push_entity(level, m.as_dyn_mut());
                }
            });
        }
    }
}
