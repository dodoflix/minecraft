//! `net.minecraft.world.entity.vehicle.minecart.NewMinecartBehavior` (server side): the rail movement of every cart when the
//! `minecart_improvements` experiment is enabled. A cart advances along the rails in steps (`stepAlongTrack`) and records one
//! [`MinecartStep`] per step; `ServerEntity` sends them as one `move_minecart_along_track` packet. The client-only interpolation
//! of those steps (`lerpClientPositionAndRotation`, `getCurrentLerpStep`) has no server counterpart.

use super::abstract_minecart::{
    self, AbstractMinecart, current_block_pos_or_rail_below, exits, get_redstone_direction,
    rail_shape,
};
use crate::server::level::ServerLevel;
use crate::world::entity::{MoverType, ServerPlayer};
use crate::world::level::block::BlockStateExt;
use crate::world::level::block::impls::base_rail_block;
use crate::world::level::{BlockGetter, LevelReader};
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::enums::RailShape;
use mc_data::{EntityType, props};
use mc_protocol::codec::Angle;
use mc_protocol::packets::play::MinecartStep;

/// `1.0E-5F` widened to a double like every `double` comparison against the float literal.
const EPSILON: f64 = 1.0E-5f32 as f64;

/// `ON_RAIL_Y_OFFSET`.
const ON_RAIL_Y_OFFSET: f64 = 0.1;

/// `OPPOSING_SLOPES_REST_AT_SPEED_THRESHOLD`.
const OPPOSING_SLOPES_REST_AT_SPEED_THRESHOLD: f64 = 0.005;

/// `getMaxSpeed(level)`: the `max_minecart_speed` rule in blocks per second (halved in water) per tick.
pub fn get_max_speed(level: &ServerLevel, in_water: bool) -> f64 {
    f64::from(level.max_minecart_speed()) * if in_water { 0.5 } else { 1.0 } / 20.0
}

/// `getSlowdownFactor()`.
pub fn get_slowdown_factor(is_vehicle: bool) -> f64 {
    if is_vehicle { 0.997 } else { 0.975 }
}

/// `new Vec3(vec3i)`.
fn vec_of(v: [i32; 3]) -> Vec3 {
    Vec3::new(f64::from(v[0]), f64::from(v[1]), f64::from(v[2]))
}

/// `isDecending(movement, shape)`.
fn is_decending(movement: Vec3, shape: RailShape) -> bool {
    match shape {
        RailShape::AscendingEast => movement.x < 0.0,
        RailShape::AscendingWest => movement.x > 0.0,
        RailShape::AscendingNorth => movement.z > 0.0,
        RailShape::AscendingSouth => movement.z < 0.0,
        _ => false,
    }
}

/// `(float)(Math.atan2(z, x) * 180.0 / Math.PI)`.
fn atan2_degrees(z: f64, x: f64) -> f32 {
    (mth::java_atan2(z, x) * 180.0 / std::f64::consts::PI) as f32
}

/// The state of one `moveAlongTrack` loop (`TrackIteration`).
struct TrackIteration {
    movement_left: f64,
    first_iteration: bool,
    has_gained_slope_speed: bool,
    has_halted: bool,
    has_boosted: bool,
}

impl TrackIteration {
    fn should_iterate(&self) -> bool {
        self.first_iteration || self.movement_left > EPSILON
    }
}

/// `tick()` (the server branch).
pub fn tick<M: AbstractMinecart + ?Sized>(m: &mut M, level: &mut ServerLevel) {
    let pos = current_block_pos_or_rail_below(m, &*level);
    let state = level.get_block_state(pos);
    if m.base().first_tick {
        m.minecart_mut().on_rails = base_rail_block::is_rail_state(state);
        adjust_to_rails(m, pos, state, true);
    }
    m.apply_gravity();
    move_along_track(m, level);
}

/// `adjustToRails(targetBlockPos, currentState, instant)`: snaps the cart onto the rail piece it is in and records the jump as
/// a step (weight 0 when `instant`).
pub fn adjust_to_rails<M: AbstractMinecart + ?Sized>(
    m: &mut M,
    target_block_pos: BlockPos,
    current_state: BlockState,
    instant: bool,
) {
    if !base_rail_block::is_rail_state(current_state) {
        return;
    }
    let shape = rail_shape(current_state);
    let (exit_a, exit_b) = exits(shape);
    let exit0 = vec_of(exit_a).scale(0.5);
    let exit1 = vec_of(exit_b).scale(0.5);
    let mut horizontal_out_direction = exit0.horizontal();
    let mut horizontal_in_direction = exit1.horizontal();
    let movement = m.base().delta_movement();
    if (movement.length() > EPSILON
        && movement.dot(horizontal_out_direction) < movement.dot(horizontal_in_direction))
        || is_decending(horizontal_in_direction, shape)
    {
        std::mem::swap(&mut horizontal_out_direction, &mut horizontal_in_direction);
    }
    let flipped_degrees = if m.minecart().flipped { 180.0f32 } else { 0.0 };
    let mut y_rot =
        180.0f32 - atan2_degrees(horizontal_out_direction.z, horizontal_out_direction.x);
    y_rot += flipped_degrees;
    let previous_position = m.base().position();
    let in_corner = exit0.x != exit1.x && exit0.z != exit1.z;
    let bottom_center = Vec3::at_bottom_center_of(&target_block_pos);
    let target_position;
    if in_corner {
        let from0_to1 = exit1.subtract(exit0);
        let from0_to_pos = previous_position.subtract(bottom_center).subtract(exit0);
        let travel_vector_from0 =
            from0_to1.scale(from0_to1.dot(from0_to_pos) / from0_to1.dot(from0_to1));
        target_position = bottom_center.add(exit0).add(travel_vector_from0);
        y_rot = 180.0f32 - atan2_degrees(travel_vector_from0.z, travel_vector_from0.x);
        y_rot += flipped_degrees;
    } else {
        let z_snap = exit0.subtract(exit1).x != 0.0;
        let x_snap = exit0.subtract(exit1).z != 0.0;
        let center = Vec3::at_center_of(&target_block_pos);
        target_position = Vec3::new(
            if x_snap {
                center.x
            } else {
                previous_position.x
            },
            f64::from(target_block_pos.y()),
            if z_snap {
                center.z
            } else {
                previous_position.z
            },
        );
    }
    let diff_from_block = target_position.subtract(previous_position);
    let moved = previous_position.add(diff_from_block);
    m.set_pos(moved.x, moved.y, moved.z);
    let mut x_rot = 0.0f32;
    let in_hill = exit0.y != exit1.y;
    let position = m.base().position();
    if in_hill {
        let in_position = bottom_center.add(horizontal_in_direction);
        let horizontal_distance_from_in = in_position.distance_to(position);
        m.set_pos(
            position.x,
            position.y + (horizontal_distance_from_in + ON_RAIL_Y_OFFSET),
            position.z,
        );
        x_rot = if m.minecart().flipped { 45.0 } else { -45.0 };
    } else {
        m.set_pos(position.x, position.y + ON_RAIL_Y_OFFSET, position.z);
    }
    set_rotation(m, y_rot, x_rot);
    let adjust_distance = previous_position.distance_to(m.base().position());
    if adjust_distance > 0.0 {
        let step = step_of(m, if instant { 0.0 } else { adjust_distance as f32 });
        m.minecart_mut().lerp_steps.push(step);
    }
}

/// `new MinecartStep(position(), getDeltaMovement(), getYRot(), getXRot(), weight)`.
fn step_of<M: AbstractMinecart + ?Sized>(m: &M, weight: f32) -> MinecartStep {
    MinecartStep {
        position: m.base().position(),
        movement: m.base().delta_movement(),
        y_rot: Angle(m.base().y_rot()),
        x_rot: Angle(m.base().x_rot()),
        weight,
    }
}

/// `setRotation(yRot, xRot)`: a turn of about 180 degrees flips the cart instead.
fn set_rotation<M: AbstractMinecart + ?Sized>(m: &mut M, mut y_rot: f32, mut x_rot: f32) {
    let y_rot_diff = f64::from((y_rot - m.base().y_rot()).abs());
    if (175.0..=185.0).contains(&y_rot_diff) {
        let flipped = m.minecart().flipped;
        m.minecart_mut().flipped = !flipped;
        y_rot -= 180.0;
        x_rot *= -1.0;
    }
    x_rot = x_rot.clamp(-45.0, 45.0);
    m.set_x_rot(x_rot % 360.0);
    m.set_y_rot(y_rot % 360.0);
}

/// `moveAlongTrack(level)`.
pub fn move_along_track<M: AbstractMinecart + ?Sized>(m: &mut M, level: &mut ServerLevel) {
    let mut iteration = TrackIteration {
        movement_left: 0.0,
        first_iteration: true,
        has_gained_slope_speed: false,
        has_halted: false,
        has_boosted: false,
    };
    while iteration.should_iterate() && m.is_alive() {
        let initial_step_delta_movement = m.base().delta_movement();
        let current_pos = current_block_pos_or_rail_below(m, &*level);
        let current_state = level.get_block_state(current_pos);
        let on_rails = base_rail_block::is_rail_state(current_state);
        if m.minecart().on_rails != on_rails {
            m.minecart_mut().on_rails = on_rails;
            adjust_to_rails(m, current_pos, current_state, false);
        }

        if on_rails {
            m.reset_fall_distance();
            m.base_mut().set_old_pos_and_rot();
            if current_state.is_block(Block::ACTIVATOR_RAIL) {
                m.activate_minecart(
                    level,
                    current_pos.x(),
                    current_pos.y(),
                    current_pos.z(),
                    current_state.get(props::POWERED),
                );
            }
            let shape = rail_shape(current_state);
            let new_delta_movement = calculate_track_speed(
                m,
                level,
                initial_step_delta_movement.horizontal(),
                &mut iteration,
                current_pos,
                current_state,
                shape,
            );
            if iteration.first_iteration {
                iteration.movement_left = new_delta_movement.horizontal_distance();
            } else {
                iteration.movement_left += new_delta_movement.horizontal_distance()
                    - initial_step_delta_movement.horizontal_distance();
            }
            m.base_mut().set_delta_movement(new_delta_movement);
            iteration.movement_left =
                step_along_track(m, level, current_pos, shape, iteration.movement_left);
        } else {
            abstract_minecart::come_off_track(m, level);
            iteration.movement_left = 0.0;
        }

        let step_position = m.base().position();
        let step_delta = step_position.subtract(m.base().old_position());
        let step_length = step_delta.length();
        if step_length > EPSILON {
            if step_delta.horizontal_distance_sqr() <= EPSILON {
                if !m.minecart().on_rails {
                    let x_rot = if m.base().on_ground() {
                        0.0
                    } else {
                        mth::rot_lerp_f32(0.2, m.base().x_rot(), 0.0)
                    };
                    m.set_x_rot(x_rot);
                }
            } else {
                let flipped = m.minecart().flipped;
                let mut y_rot = 180.0f32 - atan2_degrees(step_delta.z, step_delta.x);
                let mut x_rot = if m.base().on_ground() && !m.minecart().on_rails {
                    0.0
                } else {
                    90.0f32
                        - (mth::java_atan2(step_delta.horizontal_distance(), step_delta.y) * 180.0
                            / std::f64::consts::PI) as f32
                };
                y_rot += if flipped { 180.0 } else { 0.0 };
                x_rot *= if flipped { -1.0 } else { 1.0 };
                set_rotation(m, y_rot, x_rot);
            }
            let weight = step_length.min(get_max_speed(level, m.is_in_water())) as f32;
            let step = step_of(m, weight);
            m.minecart_mut().lerp_steps.push(step);
        } else if initial_step_delta_movement.horizontal_distance_sqr() > 0.0 {
            let step = step_of(m, 1.0);
            m.minecart_mut().lerp_steps.push(step);
        }

        if step_length > EPSILON || iteration.first_iteration {
            m.apply_effects_from_blocks(level);
            m.apply_effects_from_blocks(level);
        }
        iteration.first_iteration = false;
    }
}

/// `calculateTrackSpeed`: slope speed, rider input, halting, natural slowdown and boost, each applied at most once per
/// `moveAlongTrack` call.
fn calculate_track_speed<M: AbstractMinecart + ?Sized>(
    m: &mut M,
    level: &mut ServerLevel,
    delta_movement: Vec3,
    iteration: &mut TrackIteration,
    current_pos: BlockPos,
    current_state: BlockState,
    shape: RailShape,
) -> Vec3 {
    let mut new_delta_movement = delta_movement;
    if !iteration.has_gained_slope_speed {
        let sloped = calculate_slope_speed(m, delta_movement, shape);
        if sloped.horizontal_distance_sqr() != delta_movement.horizontal_distance_sqr() {
            iteration.has_gained_slope_speed = true;
            new_delta_movement = sloped;
        }
    }

    if iteration.first_iteration {
        let player_input = calculate_player_input_speed(m, level, new_delta_movement);
        if player_input.horizontal_distance_sqr() != new_delta_movement.horizontal_distance_sqr() {
            iteration.has_halted = true;
            new_delta_movement = player_input;
        }
    }

    if !iteration.has_halted {
        let halted = calculate_halt_track_speed(new_delta_movement, current_state);
        if halted.horizontal_distance_sqr() != new_delta_movement.horizontal_distance_sqr() {
            iteration.has_halted = true;
            new_delta_movement = halted;
        }
    }

    if iteration.first_iteration {
        new_delta_movement = m.apply_natural_slowdown(new_delta_movement);
        if new_delta_movement.length_sqr() > 0.0 {
            let speed = new_delta_movement.length().min(m.get_max_speed(level));
            new_delta_movement = new_delta_movement.normalize().scale(speed);
        }
    }

    if !iteration.has_boosted {
        let boosted =
            calculate_boost_track_speed(level, new_delta_movement, current_pos, current_state);
        if boosted.horizontal_distance_sqr() != new_delta_movement.horizontal_distance_sqr() {
            iteration.has_boosted = true;
            new_delta_movement = boosted;
        }
    }
    new_delta_movement
}

/// `calculateSlopeSpeed`.
fn calculate_slope_speed<M: AbstractMinecart + ?Sized>(
    m: &M,
    delta_movement: Vec3,
    shape: RailShape,
) -> Vec3 {
    let mut slide_speed = 0.0078125f64.max(delta_movement.horizontal_distance() * 0.02);
    if m.is_in_water() {
        slide_speed *= 0.2;
    }
    match shape {
        RailShape::AscendingEast => delta_movement.add_xyz(-slide_speed, 0.0, 0.0),
        RailShape::AscendingWest => delta_movement.add_xyz(slide_speed, 0.0, 0.0),
        RailShape::AscendingNorth => delta_movement.add_xyz(0.0, 0.0, slide_speed),
        RailShape::AscendingSouth => delta_movement.add_xyz(0.0, 0.0, -slide_speed),
        _ => delta_movement,
    }
}

/// `calculatePlayerInputSpeed`: a rider's pressed key nudges a (nearly) standing cart.
fn calculate_player_input_speed<M: AbstractMinecart + ?Sized>(
    m: &M,
    level: &ServerLevel,
    delta_movement: Vec3,
) -> Vec3 {
    let Some(player) = m
        .base()
        .passengers
        .first()
        .and_then(|&id| level.get_entity(id))
        .and_then(|rider| rider.as_any().downcast_ref::<ServerPlayer>())
    else {
        return delta_movement;
    };
    let move_intent = player.get_last_client_move_intent();
    if move_intent.length_sqr() > 0.0 {
        let rider_movement = move_intent.normalize();
        let own_dist = delta_movement.horizontal_distance_sqr();
        if rider_movement.length_sqr() > 0.0 && own_dist < 0.01 {
            return delta_movement.add(
                Vec3::new(rider_movement.x, 0.0, rider_movement.z)
                    .normalize()
                    .scale(0.001),
            );
        }
    }
    delta_movement
}

/// `calculateHaltTrackSpeed`: an unpowered powered rail brakes.
fn calculate_halt_track_speed(delta_movement: Vec3, state: BlockState) -> Vec3 {
    if state.is_block(Block::POWERED_RAIL) && !state.get(props::POWERED) {
        if delta_movement.length() < 0.03 {
            Vec3::ZERO
        } else {
            delta_movement.scale(0.5)
        }
    } else {
        delta_movement
    }
}

/// `calculateBoostTrackSpeed`: a powered powered rail accelerates, or starts a standing cart away from a solid block.
fn calculate_boost_track_speed(
    level: &ServerLevel,
    delta_movement: Vec3,
    pos: BlockPos,
    state: BlockState,
) -> Vec3 {
    if state.is_block(Block::POWERED_RAIL) && state.get(props::POWERED) {
        if delta_movement.length() > 0.01 {
            delta_movement
                .normalize()
                .scale(delta_movement.length() + 0.06)
        } else {
            let power_direction = get_redstone_direction(level, pos);
            if power_direction.length_sqr() <= 0.0 {
                delta_movement
            } else {
                power_direction.scale(delta_movement.length() + 0.2)
            }
        }
    } else {
        delta_movement
    }
}

/// `stepAlongTrack(pos, shape, movementLeft)`: moves the cart up to `movement_left` along the rail piece towards its exit and
/// returns what is left for the next piece.
pub fn step_along_track<M: AbstractMinecart + ?Sized>(
    m: &mut M,
    level: &mut ServerLevel,
    pos: BlockPos,
    shape: RailShape,
    mut movement_left: f64,
) -> f64 {
    if movement_left < EPSILON {
        return 0.0;
    }
    let old_position = m.base().position();
    let (exit_a, exit_b) = exits(shape);
    let mut movement = m.base().delta_movement().horizontal();
    if movement.length() < EPSILON {
        m.base_mut().set_delta_movement(Vec3::ZERO);
        return 0.0;
    }
    let in_hill = exit_a[1] != exit_b[1];
    let horizontal_in_direction = vec_of(exit_b).scale(0.5).horizontal();
    let mut horizontal_out_direction = vec_of(exit_a).scale(0.5).horizontal();
    if movement.dot(horizontal_out_direction) < movement.dot(horizontal_in_direction) {
        horizontal_out_direction = horizontal_in_direction;
    }
    let mut out_position = Vec3::at_bottom_center_of(&pos)
        .add(horizontal_out_direction)
        .add_xyz(0.0, ON_RAIL_Y_OFFSET, 0.0)
        .add(horizontal_out_direction.normalize().scale(EPSILON));
    if in_hill && !is_decending(movement, shape) {
        out_position = out_position.add_xyz(0.0, 1.0, 0.0);
    }
    let towards_out = out_position.subtract(m.base().position()).normalize();
    movement = towards_out.scale(movement.length() / towards_out.horizontal_distance());
    let mut new_position = old_position.add(movement.normalize().scale(
        movement_left
            * if in_hill {
                f64::from(mth::SQRT_OF_TWO)
            } else {
                1.0
            },
    ));
    if old_position.distance_to_sqr(out_position) <= old_position.distance_to_sqr(new_position) {
        movement_left = out_position.subtract(new_position).horizontal_distance();
        new_position = out_position;
    } else {
        movement_left = 0.0;
    }

    m.r#move(
        level,
        MoverType::SelfMover,
        new_position.subtract(old_position),
    );
    let new_block_state = level.get_block_state(BlockPos::containing_position(&new_position));
    if in_hill {
        if base_rail_block::is_rail_state(new_block_state) {
            let new_rail_shape = rail_shape(new_block_state);
            if rest_at_v_shape(m, shape, new_rail_shape) {
                return 0.0;
            }
        }
        let position = m.base().position();
        let horizontal_distance_from_out =
            out_position.horizontal().distance_to(position.horizontal());
        let project_y_pos = out_position.y
            + if is_decending(movement, shape) {
                horizontal_distance_from_out
            } else {
                -horizontal_distance_from_out
            };
        if position.y < project_y_pos {
            m.set_pos(position.x, project_y_pos, position.z);
        }
    }

    if m.base().position().distance_to(old_position) < EPSILON
        && new_position.distance_to(old_position) > EPSILON
    {
        m.base_mut().set_delta_movement(Vec3::ZERO);
        0.0
    } else {
        m.base_mut().set_delta_movement(movement);
        movement_left
    }
}

/// `restAtVShape`: a slow cart between two opposing slopes stops.
fn rest_at_v_shape<M: AbstractMinecart + ?Sized>(
    m: &mut M,
    current_rail_shape: RailShape,
    new_rail_shape: RailShape,
) -> bool {
    let delta_movement = m.base().delta_movement();
    if delta_movement.length_sqr() < OPPOSING_SLOPES_REST_AT_SPEED_THRESHOLD
        && base_rail_block::is_slope(new_rail_shape)
        && is_decending(delta_movement, current_rail_shape)
        && !is_decending(delta_movement, new_rail_shape)
    {
        m.base_mut().set_delta_movement(Vec3::ZERO);
        true
    } else {
        false
    }
}

/// `pushAndPickupEntities()`: `true` when the cart picked up an entity without pushing one (the caller finishes its movement).
pub fn push_and_pickup_entities<M: AbstractMinecart + ?Sized>(
    m: &mut M,
    level: &mut ServerLevel,
) -> bool {
    let hitbox = m.base().bounding_box().inflate(0.2, 0.0, 0.2);
    let picked_up = pickup_entities(m, level, &hitbox);
    if !m.base().horizontal_collision && !m.base().vertical_collision {
        return false;
    }
    let pushed = push_entities(m, level, &m.base().bounding_box().inflate_scalar(1.0E-7));
    picked_up && !pushed
}

/// `pickupEntities(hitbox)`: an empty rideable cart takes the first mob it overlaps as its rider.
fn pickup_entities<M: AbstractMinecart + ?Sized>(
    m: &mut M,
    level: &mut ServerLevel,
    hitbox: &mc_core::phys::AABB,
) -> bool {
    if !(m.is_rideable() && !m.base().is_vehicle()) {
        return false;
    }
    for id in
        crate::world::entity::living_entity::push::pushable_entities_in(level, m.as_dyn(), hitbox)
    {
        let picked_up = level.with_entity(id, |level, other| {
            !other.is_player()
                && other.get_type() != EntityType::IRON_GOLEM
                && other.as_abstract_minecart().is_none()
                && !m.base().is_vehicle()
                && !other.base().is_passenger()
                && level.start_riding_held(other, m.as_dyn_mut(), false, true)
        });
        if picked_up == Some(true) {
            return true;
        }
    }
    false
}

/// `pushEntities(hitbox)`: a rideable cart pushes players, iron golems, carts and everything while it has a rider (or when the
/// entity rides); a cart that cannot be ridden only pushes carts.
fn push_entities<M: AbstractMinecart + ?Sized>(
    m: &mut M,
    level: &mut ServerLevel,
    hitbox: &mc_core::phys::AABB,
) -> bool {
    let mut pushed = false;
    if m.is_rideable() {
        for id in crate::world::entity::living_entity::push::pushable_entities_in(
            level,
            m.as_dyn(),
            hitbox,
        ) {
            level.with_entity(id, |level, other| {
                if other.is_player()
                    || other.get_type() == EntityType::IRON_GOLEM
                    || other.as_abstract_minecart().is_some()
                    || m.base().is_vehicle()
                    || other.base().is_passenger()
                {
                    other.push_entity(level, m.as_dyn_mut());
                    pushed = true;
                }
            });
        }
    } else {
        let own = m.base().id;
        for id in level.get_entities(Some(own), hitbox, |_| true) {
            level.with_entity(id, |level, other| {
                if !m.base().passengers.contains(&id)
                    && other.is_pushable(level)
                    && other.as_abstract_minecart().is_some()
                {
                    other.push_entity(level, m.as_dyn_mut());
                    pushed = true;
                }
            });
        }
    }
    pushed
}

/// `MinecartCollisionContext`: the block below the rail the cart is on and, on a slope, the block its upper end leans on do not
/// collide with the cart.
pub fn collision_ignored_positions(
    m: &dyn AbstractMinecart,
    level: &dyn BlockGetter,
) -> (Option<BlockPos>, Option<BlockPos>) {
    let current_rail_pos = current_block_pos_or_rail_below(m, level);
    let current_state = level.get_block_state(current_rail_pos);
    if !base_rail_block::is_rail_state(current_state) {
        return (None, None);
    }
    let slope_ignore = match rail_shape(current_state) {
        RailShape::AscendingEast => Some(current_rail_pos.east()),
        RailShape::AscendingWest => Some(current_rail_pos.west()),
        RailShape::AscendingNorth => Some(current_rail_pos.north()),
        RailShape::AscendingSouth => Some(current_rail_pos.south()),
        _ => None,
    };
    (Some(current_rail_pos.below()), slope_ignore)
}

/// Whether `level` enables the `minecart_improvements` experiment (`AbstractMinecart.useExperimentalMovement(level)`).
pub fn use_experimental_movement<L: LevelReader + ?Sized>(level: &L) -> bool {
    level
        .enabled_features()
        .contains_all(mc_world::level_storage::FeatureFlagSet::MINECART_IMPROVEMENTS)
}
