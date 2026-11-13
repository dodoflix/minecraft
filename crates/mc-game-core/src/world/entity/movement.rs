//! `Entity.move` and the collision/ground logic behind it (`collide`, `collideBoundingBox`, `checkSupportingBlock`,
//! `getOnPos`, `checkFallDamage`, ...), generic over the level so it can be tested against a mock world.
//!
//! The server needs this for `MoverType.PLAYER` movement validation and for every ticking entity. Sounds, game events
//! and block `entityInside` effects of movement are not part of this module.

use super::MoverType;
use super::dimensions::EntityDimensions;
use super::entity::{Entity, MAX_MOVEMENTS_HANDLED_PER_TICK, Movement, MovementEmission};
use crate::server::level::ServerLevel;
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use crate::world::level::collision::{
    BlockGetterExt, ClipBlock, ClipContext, ClipFluid, CollisionContext, CollisionGetter,
};
use mc_core::direction::{Axis, Direction};
use mc_core::math::mth;
use mc_core::phys::shapes::{BooleanOp, Shapes, VoxelShape};
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};

/// Entity-side collision shapes of a level (`EntityGetter.getEntityCollisions(source, area)`); the source is a live
/// reference because the ticking entity is taken out of the level's storage.
pub trait EntityCollisionQuery {
    fn entity_collisions(&self, source: Option<&dyn Entity>, area: &AABB) -> Vec<VoxelShape>;
}

/// What movement needs from a level.
pub trait MovementLevel: CollisionGetter + EntityCollisionQuery {
    /// `getGameTime()` (piston movement bookkeeping).
    fn game_time(&self) -> i64;
    /// `getWorldBorder().isInsideCloseToBorder(entity, box)` collision shape, `None` when not close to the border.
    fn world_border_collision(&self, _source: &dyn Entity, _aabb: &AABB) -> Option<VoxelShape> {
        None
    }
    /// `hasChunksAt(x0, z0, x1, z1)` (block coordinates).
    fn has_chunks_at(&self, x0: i32, z0: i32, x1: i32, z1: i32) -> bool;
}

/// `CollisionContext.of(entity)` (`EntityCollisionContext(entity, false, false)`).
pub fn collision_context(entity: &dyn Entity) -> CollisionContext {
    CollisionContext::of_entity(
        entity.id(),
        entity.is_descending(),
        entity.base().y(),
        entity.held_item_for_collision(),
        entity.can_stand_on_fluid_fn(),
        false,
    )
    .with_entity_state(
        entity.base().fall_distance,
        entity.get_type() == mc_data::EntityType::FALLING_BLOCK,
        can_entity_walk_on_powder_snow(entity),
    )
}

/// `CollisionContext.of(entity)` where the cart context needs the level: a cart under the experimental movement ignores the
/// block below its rail and the block its slope leans on (`MinecartCollisionContext`).
pub fn collision_context_in(
    entity: &dyn Entity,
    level: &dyn crate::world::level::BlockGetter,
) -> CollisionContext {
    let context = collision_context(entity);
    match entity.as_abstract_minecart() {
        Some(cart) if cart.minecart().experimental => {
            let (below, slope) =
                super::vehicle::minecart::new_behavior::collision_ignored_positions(cart, level);
            context.with_ignored_positions(below, slope)
        }
        _ => context,
    }
}

/// `PowderSnowBlock.canEntityWalkOnPowderSnow(entity)`.
pub fn can_entity_walk_on_powder_snow(entity: &dyn Entity) -> bool {
    if mc_data::tags::get::<mc_data::EntityType>("powder_snow_walkable_mobs")
        .is_some_and(|t| t.contains(entity.get_type()))
    {
        return true;
    }
    entity.living().is_some() && {
        let boots = entity.get_item_by_slot(super::equipment::EquipmentSlot::Feet);
        !boots.is_empty() && boots.item() == mc_data::Item::LEATHER_BOOTS
    }
}

fn collect_colliders_ignoring_world_border<L: MovementLevel>(
    source: &dyn Entity,
    level: &L,
    entity_colliders: &[VoxelShape],
    bounding_box: &AABB,
) -> Vec<VoxelShape> {
    let mut colliders: Vec<VoxelShape> = Vec::with_capacity(entity_colliders.len() + 1);
    colliders.extend_from_slice(entity_colliders);
    if let Some(border) = level.world_border_collision(source, bounding_box) {
        colliders.push(border);
    }
    colliders.extend(
        level.get_block_collisions(Some(&collision_context_in(source, level)), bounding_box),
    );
    colliders
}

/// `Entity.collectAllColliders(source, level, box)`.
pub fn collect_all_colliders<L: MovementLevel>(
    source: &dyn Entity,
    level: &L,
    bounding_box: &AABB,
) -> Vec<VoxelShape> {
    let entity_colliders = level.entity_collisions(Some(source), bounding_box);
    collect_colliders_ignoring_world_border(source, level, &entity_colliders, bounding_box)
}

/// `Entity.collideBoundingBox(source, movement, box, level, entityColliders)`.
pub fn collide_bounding_box<L: MovementLevel>(
    source: &dyn Entity,
    movement: Vec3,
    bounding_box: &AABB,
    level: &L,
    entity_colliders: &[VoxelShape],
) -> Vec3 {
    let colliders = collect_colliders_ignoring_world_border(
        source,
        level,
        entity_colliders,
        &bounding_box.expand_towards_vec3(movement),
    );
    collide_with_shapes(movement, bounding_box, &colliders)
}

/// `Entity.collideWithShapes`.
pub fn collide_with_shapes(movement: Vec3, bounding_box: &AABB, shapes: &[VoxelShape]) -> Vec3 {
    if shapes.is_empty() {
        return movement;
    }
    let mut resolved = Vec3::ZERO;
    for axis in Direction::axis_step_order(movement) {
        let axis_movement = movement.get(axis);
        if axis_movement != 0.0 {
            let collision = Shapes::collide(
                axis,
                &bounding_box.move_vec(resolved),
                shapes,
                axis_movement,
            );
            resolved = resolved.with(axis, collision);
        }
    }
    resolved
}

/// `Entity.collectCandidateStepUpHeights`.
fn collect_candidate_step_up_heights(
    bounding_box: &AABB,
    colliders: &[VoxelShape],
    max_step_height: f32,
    step_height_to_skip: f32,
) -> Vec<f32> {
    let mut candidates: Vec<f32> = Vec::with_capacity(4);
    for collider in colliders {
        for coord in collider.get_coords(Axis::Y).to_vec() {
            let relative = (coord - bounding_box.min_y) as f32;
            if relative < 0.0 || relative == step_height_to_skip {
                continue;
            }
            if relative > max_step_height {
                break;
            }
            if !candidates.contains(&relative) {
                candidates.push(relative);
            }
        }
    }
    candidates.sort_by(|a, b| a.total_cmp(b));
    candidates
}

/// `Entity.collide(movement)`.
pub fn collide<L: MovementLevel>(entity: &dyn Entity, level: &L, movement: Vec3) -> Vec3 {
    let aabb = entity.base().bounding_box();
    let max_up_step = entity.max_up_step();
    let entity_colliders = level.entity_collisions(
        Some(entity),
        &aabb
            .expand_towards_vec3(movement)
            .expand_towards(0.0, f64::from(max_up_step), 0.0),
    );
    let movement_step = if movement.length_sqr() == 0.0 {
        movement
    } else {
        collide_bounding_box(entity, movement, &aabb, level, &entity_colliders)
    };
    let x_collision = movement.x != movement_step.x;
    let y_collision = movement.y != movement_step.y;
    let z_collision = movement.z != movement_step.z;
    let on_ground_after_collision = y_collision && movement.y < 0.0;
    if max_up_step > 0.0
        && (on_ground_after_collision || entity.base().on_ground())
        && (x_collision || z_collision)
    {
        let grounded = if on_ground_after_collision {
            aabb.move_(0.0, movement_step.y, 0.0)
        } else {
            aabb
        };
        let mut step_up = grounded.expand_towards(movement.x, f64::from(max_up_step), movement.z);
        if !on_ground_after_collision {
            step_up = step_up.expand_towards(0.0, -f64::from(1.0E-5f32), 0.0);
        }
        let colliders =
            collect_colliders_ignoring_world_border(entity, level, &entity_colliders, &step_up);
        let step_height_to_skip = movement_step.y as f32;
        for candidate in collect_candidate_step_up_heights(
            &grounded,
            &colliders,
            max_up_step,
            step_height_to_skip,
        ) {
            let step_from_ground = collide_with_shapes(
                Vec3::new(movement.x, f64::from(candidate), movement.z),
                &grounded,
                &colliders,
            );
            if step_from_ground.horizontal_distance_sqr() > movement_step.horizontal_distance_sqr()
            {
                let distance_to_ground = aabb.min_y - grounded.min_y;
                return step_from_ground.subtract(Vec3::new(0.0, distance_to_ground, 0.0));
            }
        }
    }
    movement_step
}

/// `Entity.getAvailableSpaceBelow(maxDistance)`.
pub fn get_available_space_below<L: MovementLevel>(
    entity: &dyn Entity,
    level: &L,
    max_distance: f64,
) -> f64 {
    let aabb = entity.base().bounding_box();
    let below = aabb
        .set_min_y(aabb.min_y - max_distance)
        .set_max_y(aabb.min_y);
    let colliders = collect_all_colliders(entity, level, &below);
    if colliders.is_empty() {
        max_distance
    } else {
        -Shapes::collide(Axis::Y, &aabb, &colliders, -max_distance)
    }
}

/// `Entity.checkSupportingBlock(onGround, movement)`.
pub fn check_supporting_block<L: MovementLevel>(
    entity: &mut dyn Entity,
    level: &L,
    on_ground: bool,
    movement: Option<Vec3>,
) {
    if on_ground {
        let bb = entity.base().bounding_box();
        let test_area = AABB::new(
            bb.min_x,
            bb.min_y - 1.0E-6,
            bb.min_z,
            bb.max_x,
            bb.min_y,
            bb.max_z,
        );
        let ctx = collision_context_in(entity, level);
        let position = entity.base().position();
        let mut supporting = level.find_supporting_block(&ctx, position, &test_area);
        let no_blocks = entity.base().on_ground_no_blocks;
        if supporting.is_some() || no_blocks {
            entity.base_mut().main_supporting_block_pos = supporting;
        } else if let Some(movement) = movement {
            let area = test_area.move_(-movement.x, 0.0, -movement.z);
            supporting = level.find_supporting_block(&ctx, position, &area);
            entity.base_mut().main_supporting_block_pos = supporting;
        }
        entity.base_mut().on_ground_no_blocks = supporting.is_none();
    } else {
        let b = entity.base_mut();
        b.on_ground_no_blocks = false;
        b.main_supporting_block_pos = None;
    }
}

/// `setOnGround(onGround)`.
pub fn set_on_ground<L: MovementLevel>(entity: &mut dyn Entity, level: &L, on_ground: bool) {
    entity.base_mut().set_on_ground_raw(on_ground);
    check_supporting_block(entity, level, on_ground, None);
}

/// `setOnGroundWithMovement(onGround, horizontalCollision, movement)`.
pub fn set_on_ground_with_movement<L: MovementLevel>(
    entity: &mut dyn Entity,
    level: &L,
    on_ground: bool,
    horizontal_collision: bool,
    movement: Vec3,
) {
    let b = entity.base_mut();
    b.set_on_ground_raw(on_ground);
    b.horizontal_collision = horizontal_collision;
    check_supporting_block(entity, level, on_ground, Some(movement));
}

/// `getOnPos(offset)`.
pub fn get_on_pos<L: MovementLevel>(entity: &dyn Entity, level: &L, offset: f32) -> BlockPos {
    let b = entity.base();
    let pos = b.position();
    if let Some(support) = b.main_supporting_block_pos {
        if offset <= 1.0E-5 {
            return support;
        }
        let below = level.get_block_state(support);
        let block = below.block();
        let is_fence = block.is_in_tag("fences");
        if (offset <= 0.5 && is_fence)
            || block.is_in_tag("walls")
            || block.data().classes.contains(&"FenceGateBlock")
        {
            support
        } else {
            BlockPos::new(
                support.x(),
                mth::floor_f64(pos.y - f64::from(offset)),
                support.z(),
            )
        }
    } else {
        BlockPos::new(
            mth::floor_f64(pos.x),
            mth::floor_f64(pos.y - f64::from(offset)),
            mth::floor_f64(pos.z),
        )
    }
}

/// `getOnPosLegacy()`.
pub fn get_on_pos_legacy<L: MovementLevel>(entity: &dyn Entity, level: &L) -> BlockPos {
    get_on_pos(entity, level, 0.2)
}

/// `getBlockPosBelowThatAffectsMyMovement()`.
pub fn get_block_pos_below_that_affects_my_movement<L: MovementLevel>(
    entity: &dyn Entity,
    level: &L,
) -> BlockPos {
    get_on_pos(entity, level, entity.block_pos_below_offset())
}

/// `getBlockSpeedFactor()`.
pub fn get_block_speed_factor<L: MovementLevel>(entity: &dyn Entity, level: &L) -> f32 {
    let state = level.get_block_state(entity.base().block_position());
    let here = state.block().data().speed_factor;
    if state.block() != Block::WATER && state.block() != Block::BUBBLE_COLUMN {
        if here == 1.0 {
            level
                .get_block_state(get_block_pos_below_that_affects_my_movement(entity, level))
                .block()
                .data()
                .speed_factor
        } else {
            here
        }
    } else {
        here
    }
}

/// `getBlockJumpFactor()`.
pub fn get_block_jump_factor<L: MovementLevel>(entity: &dyn Entity, level: &L) -> f32 {
    let here = level
        .get_block_state(entity.base().block_position())
        .block()
        .data()
        .jump_factor;
    let below = level
        .get_block_state(get_block_pos_below_that_affects_my_movement(entity, level))
        .block()
        .data()
        .jump_factor;
    if here == 1.0 { below } else { here }
}

/// `limitPistonMovement(vec)`.
fn limit_piston_movement<L: MovementLevel>(entity: &mut dyn Entity, level: &L, vec: Vec3) -> Vec3 {
    if vec.length_sqr() <= 1.0E-7 {
        return vec;
    }
    let game_time = level.game_time();
    let b = entity.base_mut();
    if game_time != b.piston_deltas_game_time {
        b.piston_deltas = [0.0; 3];
        b.piston_deltas_game_time = game_time;
    }
    let mut restrict = |axis: Axis, amount: f64| -> f64 {
        let i = axis as usize;
        let min = mth::clamp_f64(amount + b.piston_deltas[i], -0.51, 0.51);
        let result = min - b.piston_deltas[i];
        b.piston_deltas[i] = min;
        result
    };
    let eps = f64::from(1.0E-5f32);
    if vec.x != 0.0 {
        let xa = restrict(Axis::X, vec.x);
        if xa.abs() <= eps {
            Vec3::ZERO
        } else {
            Vec3::new(xa, 0.0, 0.0)
        }
    } else if vec.y != 0.0 {
        let ya = restrict(Axis::Y, vec.y);
        if ya.abs() <= eps {
            Vec3::ZERO
        } else {
            Vec3::new(0.0, ya, 0.0)
        }
    } else if vec.z != 0.0 {
        let za = restrict(Axis::Z, vec.z);
        if za.abs() <= eps {
            Vec3::ZERO
        } else {
            Vec3::new(0.0, 0.0, za)
        }
    } else {
        Vec3::ZERO
    }
}

/// `Entity.addMovementThisTick` (keeps at most 100 recorded movements).
fn add_movement_this_tick(entity: &mut dyn Entity, movement: Movement) {
    let q = &mut entity.base_mut().movement_this_tick;
    if q.len() >= MAX_MOVEMENTS_HANDLED_PER_TICK {
        let first = q.pop_front().expect("len >= 100");
        let second = q.pop_front().expect("len >= 100");
        q.push_front(Movement {
            from: first.from,
            to: second.to,
            axis_dependent_original_movement: None,
        });
    }
    q.push_back(movement);
}

/// `recordMovement(moverType, movement)`.
fn record_movement(entity: &mut dyn Entity, mover: MoverType, movement: Vec3) {
    if mover.is_server_and_client_simulated() || entity.move_simulation_type_is_server_and_client()
    {
        entity
            .base_mut()
            .interpolation_tracker
            .apply_predicted_movement(movement);
    }
}

/// `touchingUnloadedChunk()`.
pub fn touching_unloaded_chunk<L: MovementLevel>(entity: &dyn Entity, level: &L) -> bool {
    let bb = entity.base().bounding_box().inflate_scalar(1.0);
    let x0 = mth::floor_f64(bb.min_x);
    let x1 = mth::ceil_f64(bb.max_x);
    let z0 = mth::floor_f64(bb.min_z);
    let z1 = mth::ceil_f64(bb.max_z);
    !level.has_chunks_at(x0, z0, x1, z1)
}

/// `Entity.move(moverType, delta)`.
pub fn move_entity(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    mover: MoverType,
    mut delta: Vec3,
) {
    if mover == MoverType::ShulkerBox && entity.on_moved_by_shulker_box(level) {
        return;
    }
    if entity.base().no_physics {
        let p = entity.base().position();
        entity.set_pos(p.x + delta.x, p.y + delta.y, p.z + delta.z);
        record_movement(entity, mover, delta);
        let b = entity.base_mut();
        b.horizontal_collision = false;
        b.vertical_collision = false;
        b.vertical_collision_below = false;
        b.minor_horizontal_collision = false;
        return;
    }
    if mover == MoverType::Piston {
        delta = limit_piston_movement(entity, &*level, delta);
        if delta == Vec3::ZERO {
            return;
        }
    }
    if entity.base().stuck_speed_multiplier.length_sqr() > 1.0E-7 {
        if mover != MoverType::Piston {
            delta = delta.multiply(entity.base().stuck_speed_multiplier);
        }
        entity.base_mut().stuck_speed_multiplier = Vec3::ZERO;
        entity.base_mut().set_delta_movement(Vec3::ZERO);
    }
    delta = maybe_back_off_from_edge(entity, &*level, delta, mover);
    let movement = collide(entity, &*level, delta);
    let movement_length = movement.length_sqr();
    if movement_length > 1.0E-7 || delta.length_sqr() - movement_length < 1.0E-7 {
        if entity.base().fall_distance != 0.0 && movement_length >= 1.0 {
            let check_distance = movement.length().min(8.0);
            let from = entity.base().position();
            let check_to = from.add(movement.normalize().scale(check_distance));
            let hit = level.clip(&ClipContext::new(
                from,
                check_to,
                ClipBlock::FalldamageResetting,
                ClipFluid::Water,
                collision_context_in(entity, &*level),
            ));
            if !hit.miss {
                entity.reset_fall_distance();
            }
        }
        let pos = entity.base().position();
        let new_position = pos.add(movement);
        add_movement_this_tick(
            entity,
            Movement {
                from: pos,
                to: new_position,
                axis_dependent_original_movement: Some(delta),
            },
        );
        entity.set_pos_vec(new_position);
        record_movement(entity, mover, movement);
    }
    let x_collision = !mth::equal_f64(delta.x, movement.x);
    let z_collision = !mth::equal_f64(delta.z, movement.z);
    let horizontal_collision = x_collision || z_collision;
    entity.base_mut().horizontal_collision = horizontal_collision;
    let moved_vertically = delta.y.abs() > 0.0;
    if moved_vertically || entity.is_local_instance_authoritative() {
        let vertical = delta.y != movement.y;
        let below = vertical && delta.y < 0.0;
        entity.base_mut().vertical_collision = vertical;
        entity.base_mut().vertical_collision_below = below;
        set_on_ground_with_movement(entity, &*level, below, horizontal_collision, movement);
    }
    let minor = horizontal_collision && entity.is_horizontal_collision_minor(movement);
    entity.base_mut().minor_horizontal_collision = minor;

    let effect_pos = get_on_pos_legacy(entity, &*level);
    let effect_state = level.get_block_state(effect_pos);
    if entity.is_local_instance_authoritative() {
        let on_ground = entity.base().on_ground();
        entity.check_fall_damage(level, movement.y, on_ground, effect_state, effect_pos);
    }
    if entity.is_removed() {
        return;
    }
    if entity.can_simulate_movement()
        && ((moved_vertically && entity.base().vertical_collision) || horizontal_collision)
    {
        restitute_movement_after_collisions(
            entity,
            level,
            effect_state,
            x_collision,
            z_collision,
            movement,
        );
    }
    let emission = entity.get_movement_emission();
    if emission.emits_anything() && !entity.base().is_passenger() {
        apply_movement_emission_and_play_sound(
            entity,
            level,
            emission,
            movement,
            effect_pos,
            effect_state,
        );
    }
    let factor =
        f64::from(entity.modify_block_speed_factor(entity.get_block_speed_factor(&*level)));
    let dm = entity.base().delta_movement();
    entity
        .base_mut()
        .set_delta_movement(dm.multiply_xyz(factor, 1.0, factor));
}

/// `restituteMovementAfterCollisions`.
fn restitute_movement_after_collisions(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    effect_state: BlockState,
    x_collision: bool,
    z_collision: bool,
    movement: Vec3,
) {
    let suppressing = entity.is_suppressing_bounce();
    let mut restitution = if suppressing {
        0.0
    } else {
        entity.get_entity_bounciness()
    };
    let current = entity.base().delta_movement();
    let mut after = current;
    if x_collision {
        after = current.with(Axis::X, -current.x * restitution);
    }
    if z_collision {
        after = after.with(Axis::Z, -current.z * restitution);
    }
    let mut bounced = restitution > 0.0 && (x_collision || z_collision);
    if entity.base().vertical_collision {
        if entity.base().vertical_collision_below {
            let gravity = entity.get_effective_gravity();
            restitution = if !(-current.y <= gravity)
                && !suppressing
                && !effect_state.block().is_in_tag("suppresses_bounce")
            {
                restitution.max(block_bounciness(entity, effect_state.block()))
            } else {
                0.0
            };
        }
        let (gravity_compensation, effective_drag);
        if restitution > 0.0 {
            let portion = movement.y / current.y;
            gravity_compensation = portion * entity.get_effective_gravity();
            effective_drag = mth::lerp_f64(portion, 1.0, f64::from(entity.get_air_drag()));
            bounced = true;
        } else {
            gravity_compensation = 0.0;
            effective_drag = 1.0;
        }
        after = after.with(
            Axis::Y,
            (gravity_compensation - current.y) * effective_drag * restitution,
        );
    }
    if bounced {
        let below = get_block_pos_below_that_affects_my_movement(entity, &*level);
        let fall_distance = entity.base().fall_distance;
        effect_state.bounce_on(level, below, entity, fall_distance);
        entity.game_event(level, mc_data::GameEvent::BOUNCE);
        entity.base_mut().sync_position = true;
    }
    entity.base_mut().set_delta_movement(after);
}

/// `Entity.applyMovementEmissionAndPlaySound(emission, clippedMovement, effectPos, effectState)`.
fn apply_movement_emission_and_play_sound(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    emission: MovementEmission,
    clipped_movement: Vec3,
    effect_pos: BlockPos,
    effect_state: BlockState,
) {
    let moved_distance = (clipped_movement.length() * f64::from(0.6f32)) as f32;
    let horizontal_moved_distance =
        (clipped_movement.horizontal_distance() * f64::from(0.6f32)) as f32;
    let supporting_pos = get_on_pos(entity, &*level, 1.0E-5);
    let supporting_state = level.get_block_state(supporting_pos);
    let climbing = entity.is_state_climbable(supporting_state);
    {
        let b = entity.base_mut();
        b.move_dist += if climbing {
            moved_distance
        } else {
            horizontal_moved_distance
        };
        b.fly_dist += moved_distance;
    }
    if entity.base().move_dist > entity.base().next_step && !supporting_state.is_air() {
        let only_effect_state_emissions = supporting_pos == effect_pos;
        let mut produced_side_effects = vibration_and_sound_effects_from_block(
            entity,
            level,
            effect_pos,
            effect_state,
            emission.emits_sounds(),
            only_effect_state_emissions,
            clipped_movement,
        );
        if !only_effect_state_emissions {
            produced_side_effects |= vibration_and_sound_effects_from_block(
                entity,
                level,
                supporting_pos,
                supporting_state,
                false,
                emission.emits_events(),
                clipped_movement,
            );
        }
        if produced_side_effects {
            entity.base_mut().next_step = entity.next_step();
        } else if entity.is_in_water() {
            entity.base_mut().next_step = entity.next_step();
            if emission.emits_sounds() {
                entity.water_swim_sound(level);
            }
            if emission.emits_events() {
                let position = entity.base().position();
                level.game_event_from_live_entity(entity, mc_data::GameEvent::SWIM, position);
            }
        }
    } else if supporting_state.is_air() {
        entity.process_flapping_movement(level);
    }
}

/// `Entity.vibrationAndSoundEffectsFromBlock(pos, blockState, shouldSound, shouldVibrate, clippedMovement)`.
fn vibration_and_sound_effects_from_block(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    pos: BlockPos,
    block_state: BlockState,
    should_sound: bool,
    should_vibrate: bool,
    clipped_movement: Vec3,
) -> bool {
    if block_state.is_air() {
        return false;
    }
    let is_climbable = entity.is_state_climbable(block_state);
    if (entity.base().on_ground()
        || is_climbable
        || (entity.is_crouching() && clipped_movement.y == 0.0)
        || entity.is_on_rails())
        && !entity.is_swimming()
    {
        if should_sound {
            walking_step_sound(entity, level, pos, block_state);
        }
        if should_vibrate {
            let position = entity.base().position();
            level.game_event_from_live_entity_in_state(
                entity,
                Some(block_state),
                mc_data::GameEvent::STEP,
                position,
            );
        }
        true
    } else {
        false
    }
}

/// `Entity.walkingStepSound(onPos, onState)`.
fn walking_step_sound(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    on_pos: BlockPos,
    on_state: BlockState,
) {
    entity.play_step_sound(level, on_pos, on_state);
    let tick_count = entity.base().tick_count;
    if on_state.block().is_in_tag("crystal_sound_blocks")
        && tick_count >= entity.base().last_crystal_sound_play_tick.wrapping_add(20)
    {
        play_amethyst_step_sound(entity, level);
    }
}

/// `Entity.playAmethystStepSound()`.
fn play_amethyst_step_sound(entity: &mut dyn Entity, level: &mut ServerLevel) {
    let tick_count = entity.base().tick_count;
    let last = entity.base().last_crystal_sound_play_tick;
    let mut intensity = entity.base().crystal_sound_intensity;
    intensity *= 0.997f64.powf(f64::from(tick_count.wrapping_sub(last))) as f32;
    intensity = 1.0f32.min(intensity + 0.07);
    entity.base_mut().crystal_sound_intensity = intensity;
    let pitch = 0.5 + intensity * entity.base_mut().random.next_float() * 1.2;
    let volume = 0.1 + intensity * 1.2;
    entity.play_sound(
        level,
        mc_data::SoundEvent::BLOCK_AMETHYST_BLOCK_CHIME,
        volume,
        pitch,
    );
    entity.base_mut().last_crystal_sound_play_tick = tick_count;
}

/// `getRenderShape() == RenderShape.INVISIBLE`.
pub fn is_render_shape_invisible(state: BlockState) -> bool {
    const INVISIBLE: [&str; 9] = [
        "AirBlock",
        "BarrierBlock",
        "BubbleColumnBlock",
        "EndGatewayBlock",
        "EndPortalBlock",
        "LightBlock",
        "LiquidBlock",
        "StructureVoidBlock",
        "MovingPistonBlock",
    ];
    state
        .block()
        .data()
        .classes
        .iter()
        .any(|c| INVISIBLE.contains(c))
}

/// `Level.containsAnyLiquid(box)`.
pub fn contains_any_liquid<L: BlockGetter + ?Sized>(level: &L, bx: &AABB) -> bool {
    let (x0, x1) = (mth::floor_f64(bx.min_x), mth::ceil_f64(bx.max_x));
    let (y0, y1) = (mth::floor_f64(bx.min_y), mth::ceil_f64(bx.max_y));
    let (z0, z1) = (mth::floor_f64(bx.min_z), mth::ceil_f64(bx.max_z));
    for x in x0..x1 {
        for y in y0..y1 {
            for z in z0..z1 {
                if !level
                    .get_block_state(BlockPos::new(x, y, z))
                    .get_fluid_state()
                    .is_empty()
                {
                    return true;
                }
            }
        }
    }
    false
}

/// `Entity.isInWall()`.
pub fn is_in_wall(entity: &dyn Entity, level: &dyn BlockGetter) -> bool {
    if entity.base().no_physics {
        return false;
    }
    let check_width = entity.base().dimensions.width * 0.8;
    let eye_bb = AABB::of_size(
        entity.base().get_eye_position(),
        f64::from(check_width),
        1.0E-6,
        f64::from(check_width),
    );
    BlockPos::between_closed_aabb(&eye_bb).any(|pos| {
        let state = level.get_block_state(pos);
        !state.is_air()
            && state.is_suffocating_at(level, pos)
            && Shapes::join_is_not_empty(
                &state.get_collision_shape(level, pos).move_vec(Vec3::new(
                    f64::from(pos.x()),
                    f64::from(pos.y()),
                    f64::from(pos.z()),
                )),
                &Shapes::create(eye_bb),
                BooleanOp::AND,
            )
    })
}

fn block_bounciness(entity: &dyn Entity, block: Block) -> f64 {
    let mut b = block.data().bounce_restitution;
    if entity.living().is_none() {
        b *= 0.8;
    }
    f64::from(b)
}

/// `Player.maybeBackOffFromEdge` (entities without edge protection return `delta` unchanged).
fn maybe_back_off_from_edge<L: MovementLevel>(
    entity: &dyn Entity,
    level: &L,
    delta: Vec3,
    mover: MoverType,
) -> Vec3 {
    let max_down_step = entity.max_up_step();
    if !(entity.stays_on_ground_surface_at_edges()
        && !(delta.y > 0.0)
        && (mover == MoverType::SelfMover || mover == MoverType::Player)
        && is_above_ground(entity, level, max_down_step))
    {
        return delta;
    }
    let (mut delta_x, mut delta_z) = (delta.x, delta.z);
    let step_x = delta_x.signum_java() * 0.05;
    let step_z = delta_z.signum_java() * 0.05;
    while delta_x != 0.0 && can_fall_at_least(entity, level, delta_x, 0.0, f64::from(max_down_step))
    {
        if delta_x.abs() <= 0.05 {
            delta_x = 0.0;
            break;
        }
        delta_x -= step_x;
    }
    while delta_z != 0.0 && can_fall_at_least(entity, level, 0.0, delta_z, f64::from(max_down_step))
    {
        if delta_z.abs() <= 0.05 {
            delta_z = 0.0;
            break;
        }
        delta_z -= step_z;
    }
    while delta_x != 0.0
        && delta_z != 0.0
        && can_fall_at_least(entity, level, delta_x, delta_z, f64::from(max_down_step))
    {
        if delta_x.abs() <= 0.05 {
            delta_x = 0.0;
        } else {
            delta_x -= step_x;
        }
        if delta_z.abs() <= 0.05 {
            delta_z = 0.0;
        } else {
            delta_z -= step_z;
        }
    }
    Vec3::new(delta_x, delta.y, delta_z)
}

fn is_above_ground<L: MovementLevel>(entity: &dyn Entity, level: &L, max_down_step: f32) -> bool {
    let b = entity.base();
    b.on_ground()
        || (b.fall_distance < f64::from(max_down_step)
            && !can_fall_at_least(
                entity,
                level,
                0.0,
                0.0,
                f64::from(max_down_step) - b.fall_distance,
            ))
}

fn can_fall_at_least<L: MovementLevel>(
    entity: &dyn Entity,
    level: &L,
    delta_x: f64,
    delta_z: f64,
    min_height: f64,
) -> bool {
    let bb = entity.base().bounding_box();
    let area = AABB::new(
        bb.min_x + 1.0E-7 + delta_x,
        bb.min_y - min_height - 1.0E-7,
        bb.min_z + 1.0E-7 + delta_z,
        bb.max_x - 1.0E-7 + delta_x,
        bb.min_y,
        bb.max_z - 1.0E-7 + delta_z,
    );
    level.no_collision(
        Some(entity.id()),
        Some(&collision_context_in(entity, level)),
        &area,
    )
}

trait SignumJava {
    fn signum_java(self) -> f64;
}

impl SignumJava for f64 {
    /// `Math.signum(double)`: 0.0/-0.0/NaN unchanged.
    fn signum_java(self) -> f64 {
        if self == 0.0 || self.is_nan() {
            self
        } else {
            self.signum()
        }
    }
}
/// `Entity.fudgePositionAfterSizeChange(previousDimensions)`.
pub fn fudge_position_after_size_change(
    e: &mut dyn Entity,
    level: &ServerLevel,
    previous: &EntityDimensions,
) -> bool {
    let new_dimensions = e.get_dimensions(e.base().get_pose());
    let (new_width, new_height) = (
        f64::from(new_dimensions.width),
        f64::from(new_dimensions.height),
    );
    let (prev_width, prev_height) = (f64::from(previous.width), f64::from(previous.height));
    let old_center = e.position().add(Vec3::new(0.0, prev_height / 2.0, 0.0));
    let width_delta = f64::from((new_dimensions.width - previous.width).max(0.0)) + 1.0E-6;
    let height_delta = f64::from((new_dimensions.height - previous.height).max(0.0)) + 1.0E-6;
    let allowed_centers = Shapes::create(AABB::of_size(
        old_center,
        width_delta,
        height_delta,
        width_delta,
    ));
    if let Some(free) = level.find_free_position_for(
        e,
        &allowed_centers,
        old_center,
        new_width,
        new_height,
        new_width,
    ) {
        let p = free.add(Vec3::new(0.0, -new_height / 2.0, 0.0));
        e.set_pos(p.x, p.y, p.z);
        return true;
    }
    if new_width > prev_width && new_height > prev_height {
        let allowed_centers_ignoring_y =
            Shapes::create(AABB::of_size(old_center, width_delta, 1.0E-6, width_delta));
        if let Some(free) = level.find_free_position_for(
            e,
            &allowed_centers_ignoring_y,
            old_center,
            new_width,
            prev_height,
            new_width,
        ) {
            let p = free.add(Vec3::new(0.0, -prev_height / 2.0 + 1.0E-6, 0.0));
            e.set_pos(p.x, p.y, p.z);
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collide_with_shapes_stops_a_falling_box_on_the_floor() {
        let bb = AABB::new(0.0, 1.5, 0.0, 0.6, 3.3, 0.6);
        let floor = Shapes::create(AABB::new(-1.0, 0.0, -1.0, 2.0, 1.0, 2.0));
        let r = collide_with_shapes(Vec3::new(0.0, -1.0, 0.0), &bb, &[floor]);
        assert_eq!(r, Vec3::new(0.0, -0.5, 0.0));
    }

    #[test]
    fn collide_with_shapes_resolves_axes_in_vanilla_order() {
        // moving diagonally into a wall on +x: y first, then x (|x| >= |z|), x is clipped, z passes
        let bb = AABB::new(0.0, 0.0, 0.0, 0.6, 1.8, 0.6);
        let wall = Shapes::create(AABB::new(1.0, 0.0, -1.0, 2.0, 2.0, 2.0));
        let r = collide_with_shapes(Vec3::new(1.0, 0.0, 0.5), &bb, &[wall]);
        assert!((r.x - 0.4).abs() < 1e-9 && r.z == 0.5);
    }

    #[test]
    fn collide_with_shapes_steps_z_before_x_when_x_is_smaller() {
        // |x| < |z|: z is resolved first at the original x, where the box still overlaps the wall's x range, so z is blocked
        // although the later x movement would clear the wall
        let bb = AABB::new(0.0, 0.0, 0.0, 0.6, 1.8, 0.6);
        let wall = Shapes::create(AABB::new(0.58, 0.0, -1.0, 2.0, 2.0, 0.0));
        let r = collide_with_shapes(Vec3::new(-0.04, -0.08, -0.05), &bb, &[wall]);
        assert_eq!(r, Vec3::new(-0.04, -0.08, 0.0));
    }
}
