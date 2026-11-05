//! `LivingEntity.travel` and everything below it: ground/air, water, lava, gliding, ladders, friction.

use super::util::{self, FluidKind, do_move, has_effect, is_in_lava, is_in_liquid, is_in_water, is_sprinting, l, look_angle, move_relative};
use crate::server::level::ServerLevel;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::{Entity, MoverType, movement};
use crate::world::level::BlockGetter;
use crate::world::level::block::helpers;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_data::block::Block;
use mc_data::{Attribute, MobEffect};

/// `computeModifiedFriction(friction, modifier)`.
pub fn compute_modified_friction(friction: f32, modifier: f32) -> f32 {
    mth::clamp_f32(1.0 - (1.0 - friction) * modifier, 0.0, 1.0)
}

/// `LivingEntity.getEffectiveGravity()`.
pub fn living_effective_gravity(entity: &dyn Entity) -> f64 {
    let is_falling = entity.base().delta_movement().y <= 0.0;
    if is_falling && has_effect(entity, MobEffect::SLOW_FALLING) { entity.get_gravity().min(0.01) } else { entity.get_gravity() }
}

/// `LivingEntity.onClimbable()` (see `Entity::on_climbable`; the `lastClimbablePos` write is done by [`climbable`]).
pub fn on_climbable(entity: &dyn Entity, level: &ServerLevel) -> bool {
    if entity.is_spectator() {
        return false;
    }
    let pos = entity.base().block_position();
    let state = level.get_block_state(pos);
    if entity.base().get_shared_flag(crate::world::entity::entity::FLAG_FALL_FLYING) && state.block().is_in_tag("can_glide_through") {
        false
    } else if state.block().is_in_tag("climbable") {
        true
    } else if helpers::is_instance_of(state, "TrapDoorBlock") && trapdoor_usable_as_ladder(level, pos, state) {
        true
    } else {
        false
    }
}

/// `trapdoorUsableAsLadder(pos, state)`.
fn trapdoor_usable_as_ladder(level: &ServerLevel, pos: mc_core::pos::BlockPos, state: mc_data::block::BlockState) -> bool {
    use mc_data::props;
    if !state.get(props::OPEN) {
        return false;
    }
    let below = level.get_block_state(pos.below());
    below.block() == Block::LADDER && below.get(props::HORIZONTAL_FACING) == state.get(props::HORIZONTAL_FACING)
}

/// `onClimbable()` as called by the entity's own movement: dispatches the virtual and records `lastClimbablePos`.
pub fn climbable(entity: &mut dyn Entity, level: &ServerLevel) -> bool {
    let climbing = entity.on_climbable(level);
    if climbing {
        let pos = entity.base().block_position();
        util::lm(entity).last_climbable_pos = Some(pos);
    }
    climbing
}

/// `LivingEntity.getFlyingSpeed()`.
pub fn get_flying_speed(entity: &dyn Entity, level: &ServerLevel) -> f32 {
    let controller_is_player = entity.get_controlling_passenger().and_then(|id| level.get_entity(id)).is_some_and(|e| e.is_player());
    if controller_is_player { entity.get_speed() * 0.1 } else { 0.02 }
}

/// `shouldTravelInFluid(fluidState)`.
fn should_travel_in_fluid(entity: &dyn Entity, level: &ServerLevel) -> bool {
    if !(is_in_liquid(entity) && entity.is_affected_by_fluids()) {
        return false;
    }
    let fluid = level.get_fluid_state(entity.base().block_position());
    !entity.can_stand_on_fluid_fn().is_some_and(|can_stand| can_stand(fluid.fluid()))
}

/// `LivingEntity.travel(input)`.
pub fn travel(entity: &mut dyn Entity, level: &mut ServerLevel, input: Vec3) {
    if should_travel_in_fluid(entity, level) {
        entity.travel_in_fluid(level, input);
    } else if l(entity).is_fall_flying() {
        travel_fall_flying(entity, level, input);
    } else {
        travel_in_air(entity, level, input);
    }
}

/// `travelFlying(input, speed)`.
pub fn travel_flying(entity: &mut dyn Entity, level: &mut ServerLevel, input: Vec3, speed: f32) {
    travel_flying_speeds(entity, level, input, 0.02, 0.02, speed);
}

/// `travelFlying(input, waterSpeed, lavaSpeed, airSpeed)`.
pub fn travel_flying_speeds(entity: &mut dyn Entity, level: &mut ServerLevel, input: Vec3, water_speed: f32, lava_speed: f32, air_speed: f32) {
    if is_in_water(entity) {
        move_relative(entity, water_speed, input);
        let dm = entity.base().delta_movement();
        do_move(entity, level, MoverType::SelfMover, dm);
        let dm = entity.base().delta_movement();
        entity.base_mut().set_delta_movement(dm.scale(f64::from(0.8f32)));
    } else if is_in_lava(entity) {
        move_relative(entity, lava_speed, input);
        let dm = entity.base().delta_movement();
        do_move(entity, level, MoverType::SelfMover, dm);
        let dm = entity.base().delta_movement();
        entity.base_mut().set_delta_movement(dm.scale(0.5));
    } else {
        move_relative(entity, air_speed, input);
        let dm = entity.base().delta_movement();
        do_move(entity, level, MoverType::SelfMover, dm);
        let dm = entity.base().delta_movement();
        entity.base_mut().set_delta_movement(dm.scale(f64::from(0.91f32)));
    }
}

/// `travelInAir(input)`.
fn travel_in_air(entity: &mut dyn Entity, level: &mut ServerLevel, input: Vec3) {
    let pos_below = movement::get_block_pos_below_that_affects_my_movement(entity, level);
    let block_friction = if entity.base().on_ground() {
        compute_modified_friction(level.get_block_state(pos_below).block().data().friction, l(entity).get_attribute_value(Attribute::FRICTION_MODIFIER) as f32)
    } else {
        1.0
    };
    let movement = handle_relative_friction_and_calculate_movement(entity, level, input, block_friction);
    let mut movement_y = movement.y;
    if let Some(amplifier) = util::effect_amplifier(entity, MobEffect::LEVITATION) {
        movement_y += (0.05 * f64::from(amplifier + 1) - movement.y) * 0.2;
    } else {
        // server side: `!level.isClientSide()`
        movement_y -= entity.get_effective_gravity();
    }
    if l(entity).discard_friction {
        entity.base_mut().set_delta_movement_xyz(movement.x, movement_y, movement.z);
    } else {
        let air_drag_modifier = l(entity).get_attribute_value(Attribute::AIR_DRAG_MODIFIER) as f32;
        let air_drag = compute_modified_friction(0.91, air_drag_modifier);
        let friction = block_friction * air_drag;
        let vertical_friction = if entity.omnidirectional_air_mover() { air_drag } else { compute_modified_friction(0.98, air_drag_modifier) };
        entity.base_mut().set_delta_movement_xyz(movement.x * f64::from(friction), movement_y * f64::from(vertical_friction), movement.z * f64::from(friction));
    }
}

/// `travelInFluid(input)`.
pub fn travel_in_fluid(entity: &mut dyn Entity, level: &mut ServerLevel, input: Vec3) {
    let is_falling = entity.base().delta_movement().y <= 0.0;
    let old_y = entity.base().y();
    let base_gravity = entity.get_effective_gravity();
    if is_in_water(entity) {
        entity.travel_in_water(level, input, base_gravity, is_falling, old_y);
    } else {
        travel_in_lava(entity, level, input, base_gravity, is_falling, old_y);
    }
    float_in_liquid_while_ridden(entity);
}

/// `travelInWater(input, baseGravity, isFalling, oldY)`.
pub fn travel_in_water(entity: &mut dyn Entity, level: &mut ServerLevel, input: Vec3, base_gravity: f64, is_falling: bool, old_y: f64) {
    let mut slow_down = if is_sprinting(entity) { 0.9 } else { entity.get_water_slow_down() };
    let mut speed = 0.02f32;
    let mut water_walker = l(entity).get_attribute_value(Attribute::WATER_MOVEMENT_EFFICIENCY) as f32;
    if !entity.base().on_ground() {
        water_walker *= 0.5;
    }
    if water_walker > 0.0 {
        slow_down += (0.54600006f32 - slow_down) * water_walker;
        speed += (entity.get_speed() - speed) * water_walker;
    }
    if has_effect(entity, MobEffect::DOLPHINS_GRACE) {
        slow_down = 0.96;
    }
    move_relative(entity, speed, input);
    let dm = entity.base().delta_movement();
    do_move(entity, level, MoverType::SelfMover, dm);
    let mut movement = entity.base().delta_movement();
    if entity.base().horizontal_collision && climbable(entity, level) {
        movement = Vec3::new(movement.x, 0.2, movement.z);
    }
    movement = movement.multiply_xyz(f64::from(slow_down), f64::from(0.8f32), f64::from(slow_down));
    let adjusted = get_fluid_falling_adjusted_movement(entity, base_gravity, is_falling, movement);
    entity.base_mut().set_delta_movement(adjusted);
    jump_out_of_fluid(entity, level, old_y);
}

/// `isInShallowFluid(tag)`.
pub fn is_in_shallow_fluid(entity: &dyn Entity, fluid: FluidKind) -> bool {
    util::get_fluid_height(entity, fluid) <= entity.get_fluid_jump_threshold()
}

/// `isInFluidDeeperThan(depth, tag)`.
pub fn is_in_fluid_deeper_than(entity: &dyn Entity, depth: f64, fluid: FluidKind) -> bool {
    util::get_fluid_height(entity, fluid) > depth
}

/// `travelInLava(input, baseGravity, isFalling, oldY)`.
fn travel_in_lava(entity: &mut dyn Entity, level: &mut ServerLevel, input: Vec3, base_gravity: f64, is_falling: bool, old_y: f64) {
    move_relative(entity, 0.02, input);
    let dm = entity.base().delta_movement();
    do_move(entity, level, MoverType::SelfMover, dm);
    if is_in_shallow_fluid(entity, FluidKind::Lava) {
        let dm = entity.base().delta_movement().multiply_xyz(0.5, f64::from(0.8f32), 0.5);
        entity.base_mut().set_delta_movement(dm);
        let dm = entity.base().delta_movement();
        let movement = get_fluid_falling_adjusted_movement(entity, base_gravity, is_falling, dm);
        entity.base_mut().set_delta_movement(movement);
    } else {
        let dm = entity.base().delta_movement().scale(0.5);
        entity.base_mut().set_delta_movement(dm);
    }
    if base_gravity != 0.0 {
        let dm = entity.base().delta_movement().add_xyz(0.0, -base_gravity / 4.0, 0.0);
        entity.base_mut().set_delta_movement(dm);
    }
    jump_out_of_fluid(entity, level, old_y);
}

/// `jumpOutOfFluid(oldY)`.
fn jump_out_of_fluid(entity: &mut dyn Entity, level: &ServerLevel, old_y: f64) {
    let movement = entity.base().delta_movement();
    let y = entity.base().y();
    if entity.base().horizontal_collision && util::is_free(level, entity, movement.x, movement.y + f64::from(0.6f32) - y + old_y, movement.z) {
        entity.base_mut().set_delta_movement_xyz(movement.x, f64::from(0.3f32), movement.z);
    }
}

/// `floatInLiquidWhileRidden(FluidTags.ENTITY_FLOATABLE)` (`entity_floatable` = water).
fn float_in_liquid_while_ridden(entity: &mut dyn Entity) {
    let floats = mc_data::tags::get::<mc_data::EntityType>("can_float_while_ridden").is_some_and(|t| t.contains(entity.get_type()));
    if entity.base().is_vehicle() && floats && is_in_fluid_deeper_than(entity, entity.get_fluid_jump_threshold(), FluidKind::Water) {
        let dm = entity.base().delta_movement().add_xyz(0.0, f64::from(0.04f32), 0.0);
        entity.base_mut().set_delta_movement(dm);
    }
}

/// `travelFallFlying(input)`.
fn travel_fall_flying(entity: &mut dyn Entity, level: &mut ServerLevel, input: Vec3) {
    if climbable(entity, level) {
        travel_in_air(entity, level, input);
        super::fall_flying::stop_fall_flying(entity);
    } else {
        let last_movement = entity.base().delta_movement();
        let last_speed = last_movement.horizontal_distance();
        let updated = update_fall_flying_movement(entity, last_movement);
        entity.base_mut().set_delta_movement(updated);
        let dm = entity.base().delta_movement();
        do_move(entity, level, MoverType::SelfMover, dm);
        let new_speed = entity.base().delta_movement().horizontal_distance();
        handle_fall_flying_collisions(entity, level, last_speed, new_speed);
    }
}

/// `updateFallFlyingMovement(movement)`.
fn update_fall_flying_movement(entity: &dyn Entity, mut movement: Vec3) -> Vec3 {
    let look = look_angle(entity);
    let lean_angle = entity.base().x_rot() * (std::f64::consts::PI / 180.0) as f32;
    let look_hor_length = (look.x * look.x + look.z * look.z).sqrt();
    let move_hor_length = movement.horizontal_distance();
    let gravity = entity.get_effective_gravity();
    let cos = f64::from(lean_angle).cos();
    let lift_force = cos * cos;
    movement = movement.add_xyz(0.0, gravity * (-1.0 + lift_force * 0.75), 0.0);
    if movement.y < 0.0 && look_hor_length > 0.0 {
        let convert = movement.y * -0.1 * lift_force;
        movement = movement.add_xyz(look.x * convert / look_hor_length, convert, look.z * convert / look_hor_length);
    }
    if lean_angle < 0.0 && look_hor_length > 0.0 {
        let convert = move_hor_length * f64::from(-mth::sin(f64::from(lean_angle))) * 0.04;
        movement = movement.add_xyz(-look.x * convert / look_hor_length, convert * 3.2, -look.z * convert / look_hor_length);
    }
    if look_hor_length > 0.0 {
        movement = movement.add_xyz((look.x / look_hor_length * move_hor_length - movement.x) * 0.1, 0.0, (look.z / look_hor_length * move_hor_length - movement.z) * 0.1);
    }
    movement.multiply_xyz(f64::from(0.99f32), f64::from(0.98f32), f64::from(0.99f32))
}

/// `handleFallFlyingCollisions(moveHorLength, newMoveHorLength)`.
fn handle_fall_flying_collisions(entity: &mut dyn Entity, level: &mut ServerLevel, move_hor_length: f64, new_move_hor_length: f64) {
    if entity.base().horizontal_collision {
        let diff = move_hor_length - new_move_hor_length;
        let dmg = (diff * 10.0 - 3.0) as f32;
        if dmg > 0.0 {
            util::play_sound(level, entity, util::fall_damage_sound(entity, dmg as i32), 1.0, 1.0);
            util::hurt(level, entity, |sources| sources.fly_into_wall(), dmg);
        }
    }
}

/// `travelRidden(controller, selfInput)`.
pub fn travel_ridden(entity: &mut dyn Entity, level: &mut ServerLevel, controller: crate::world::entity::EntityId, self_input: Vec3) {
    let ridden_input = entity.get_ridden_input(level, controller, self_input);
    entity.tick_ridden(level, controller, ridden_input);
    if entity.can_simulate_movement() {
        let speed = entity.get_ridden_speed(level, controller);
        entity.set_speed(speed);
        entity.travel(level, ridden_input);
    } else {
        entity.base_mut().set_delta_movement(Vec3::ZERO);
    }
}

/// `PowderSnowBlock.canEntityWalkOnPowderSnow(entity)`.
fn can_entity_walk_on_powder_snow(entity: &dyn Entity) -> bool {
    if mc_data::tags::get::<mc_data::EntityType>("powder_snow_walkable_mobs").is_some_and(|t| t.contains(entity.get_type())) {
        true
    } else if entity.living().is_some() {
        entity.get_item_by_slot(EquipmentSlot::Feet).item() == mc_data::Item::LEATHER_BOOTS
    } else {
        false
    }
}

/// `handleRelativeFrictionAndCalculateMovement(input, friction)`.
fn handle_relative_friction_and_calculate_movement(entity: &mut dyn Entity, level: &mut ServerLevel, input: Vec3, friction: f32) -> Vec3 {
    let speed = get_friction_influenced_speed(entity, level, friction);
    move_relative(entity, speed, input);
    let dm = entity.base().delta_movement();
    let climbed = handle_on_climbable(entity, level, dm);
    entity.base_mut().set_delta_movement(climbed);
    let dm = entity.base().delta_movement();
    do_move(entity, level, MoverType::SelfMover, dm);
    let mut movement = entity.base().delta_movement();
    if (entity.base().horizontal_collision || l(entity).jumping) && (climbable(entity, level) || entity.base().was_in_powder_snow && can_entity_walk_on_powder_snow(entity)) {
        movement = Vec3::new(movement.x, 0.2, movement.z);
    }
    movement
}

/// `getFluidFallingAdjustedMovement(baseGravity, isFalling, movement)`.
pub fn get_fluid_falling_adjusted_movement(entity: &dyn Entity, base_gravity: f64, is_falling: bool, movement: Vec3) -> Vec3 {
    if base_gravity != 0.0 && !is_sprinting(entity) {
        let yd = if is_falling && (movement.y - 0.005).abs() >= 0.003 && (movement.y - base_gravity / 16.0).abs() < 0.003 {
            -0.003
        } else {
            movement.y - base_gravity / 16.0
        };
        Vec3::new(movement.x, yd, movement.z)
    } else {
        movement
    }
}

/// `handleOnClimbable(delta)`.
fn handle_on_climbable(entity: &mut dyn Entity, level: &ServerLevel, mut delta: Vec3) -> Vec3 {
    if climbable(entity, level) {
        entity.reset_fall_distance();
        let max = f64::from(0.15f32);
        let xd = mth::clamp_f64(delta.x, -max, max);
        let zd = mth::clamp_f64(delta.z, -max, max);
        let mut yd = delta.y.max(-max);
        if yd < 0.0
            && level.get_block_state(entity.base().block_position()).block() != Block::SCAFFOLDING
            && entity.is_suppressing_sliding_down_ladder()
            && entity.is_player()
        {
            yd = 0.0;
        }
        delta = Vec3::new(xd, yd, zd);
    }
    delta
}

/// `getFrictionInfluencedSpeed(blockFriction)`.
fn get_friction_influenced_speed(entity: &dyn Entity, level: &ServerLevel, block_friction: f32) -> f32 {
    if entity.base().on_ground() {
        if f64::from(block_friction) > 0.6 { entity.get_speed() * (0.21600002f32 / (block_friction * block_friction * block_friction)) } else { entity.get_speed() }
    } else {
        entity.get_flying_speed(level)
    }
}
