//! `Entity.move` and the collision/ground logic behind it (`collide`, `collideBoundingBox`, `checkSupportingBlock`,
//! `getOnPos`, `checkFallDamage`, ...), generic over the level so it can be tested against a mock world.
//!
//! The server needs this for `MoverType.PLAYER` movement validation and for every ticking entity. Sounds, game events
//! and block `entityInside` effects of movement are not part of this module (see `STATUS.md` gaps).

use super::entity::{Entity, Movement, MAX_MOVEMENTS_HANDLED_PER_TICK};
use super::MoverType;
use crate::world::level::collision::{BlockGetterExt, ClipBlock, ClipContext, ClipFluid, CollisionContext, CollisionGetter};
use mc_core::direction::Axis;
use mc_core::math::mth;
use mc_core::phys::shapes::{Shapes, VoxelShape};
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
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
    CollisionContext::of_entity(entity.id(), entity.is_descending(), entity.base().y(), entity.held_item_for_collision(), entity.can_stand_on_fluid_fn(), false)
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
    colliders.extend(level.get_block_collisions(Some(&collision_context(source)), bounding_box));
    colliders
}

/// `Entity.collectAllColliders(source, level, box)`.
pub fn collect_all_colliders<L: MovementLevel>(source: &dyn Entity, level: &L, bounding_box: &AABB) -> Vec<VoxelShape> {
    let entity_colliders = level.entity_collisions(Some(source), bounding_box);
    collect_colliders_ignoring_world_border(source, level, &entity_colliders, bounding_box)
}

/// `Entity.collideBoundingBox(source, movement, box, level, entityColliders)`.
pub fn collide_bounding_box<L: MovementLevel>(source: &dyn Entity, movement: Vec3, bounding_box: &AABB, level: &L, entity_colliders: &[VoxelShape]) -> Vec3 {
    let colliders = collect_colliders_ignoring_world_border(source, level, entity_colliders, &bounding_box.expand_towards_vec3(movement));
    collide_with_shapes(movement, bounding_box, &colliders)
}

/// `Direction.axisStepOrder(movement)`.
fn axis_step_order(movement: Vec3) -> [Axis; 3] {
    if movement.y.abs() < movement.z.abs() {
        [Axis::Y, Axis::Z, Axis::X]
    } else {
        [Axis::Y, Axis::X, Axis::Z]
    }
}

/// `Entity.collideWithShapes`.
pub fn collide_with_shapes(movement: Vec3, bounding_box: &AABB, shapes: &[VoxelShape]) -> Vec3 {
    if shapes.is_empty() {
        return movement;
    }
    let mut resolved = Vec3::ZERO;
    for axis in axis_step_order(movement) {
        let axis_movement = movement.get(axis);
        if axis_movement != 0.0 {
            let collision = Shapes::collide(axis, &bounding_box.move_vec(resolved), shapes, axis_movement);
            resolved = resolved.with(axis, collision);
        }
    }
    resolved
}

/// `Entity.collectCandidateStepUpHeights`.
fn collect_candidate_step_up_heights(bounding_box: &AABB, colliders: &[VoxelShape], max_step_height: f32, step_height_to_skip: f32) -> Vec<f32> {
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
    let entity_colliders = level.entity_collisions(Some(entity), &aabb.expand_towards_vec3(movement).expand_towards(0.0, f64::from(max_up_step), 0.0));
    let movement_step = if movement.length_sqr() == 0.0 { movement } else { collide_bounding_box(entity, movement, &aabb, level, &entity_colliders) };
    let x_collision = movement.x != movement_step.x;
    let y_collision = movement.y != movement_step.y;
    let z_collision = movement.z != movement_step.z;
    let on_ground_after_collision = y_collision && movement.y < 0.0;
    if max_up_step > 0.0 && (on_ground_after_collision || entity.base().on_ground()) && (x_collision || z_collision) {
        let grounded = if on_ground_after_collision { aabb.move_(0.0, movement_step.y, 0.0) } else { aabb };
        let mut step_up = grounded.expand_towards(movement.x, f64::from(max_up_step), movement.z);
        if !on_ground_after_collision {
            step_up = step_up.expand_towards(0.0, -f64::from(1.0E-5f32), 0.0);
        }
        let colliders = collect_colliders_ignoring_world_border(entity, level, &entity_colliders, &step_up);
        let step_height_to_skip = movement_step.y as f32;
        for candidate in collect_candidate_step_up_heights(&grounded, &colliders, max_up_step, step_height_to_skip) {
            let step_from_ground = collide_with_shapes(Vec3::new(movement.x, f64::from(candidate), movement.z), &grounded, &colliders);
            if step_from_ground.horizontal_distance_sqr() > movement_step.horizontal_distance_sqr() {
                let distance_to_ground = aabb.min_y - grounded.min_y;
                return step_from_ground.subtract(Vec3::new(0.0, distance_to_ground, 0.0));
            }
        }
    }
    movement_step
}

/// `Entity.getAvailableSpaceBelow(maxDistance)`.
pub fn get_available_space_below<L: MovementLevel>(entity: &dyn Entity, level: &L, max_distance: f64) -> f64 {
    let aabb = entity.base().bounding_box();
    let below = aabb.set_min_y(aabb.min_y - max_distance).set_max_y(aabb.min_y);
    let colliders = collect_all_colliders(entity, level, &below);
    if colliders.is_empty() { max_distance } else { -Shapes::collide(Axis::Y, &aabb, &colliders, -max_distance) }
}

/// `Entity.checkSupportingBlock(onGround, movement)`.
pub fn check_supporting_block<L: MovementLevel>(entity: &mut dyn Entity, level: &L, on_ground: bool, movement: Option<Vec3>) {
    if on_ground {
        let bb = entity.base().bounding_box();
        let test_area = AABB::new(bb.min_x, bb.min_y - 1.0E-6, bb.min_z, bb.max_x, bb.min_y, bb.max_z);
        let ctx = collision_context(entity);
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
pub fn set_on_ground_with_movement<L: MovementLevel>(entity: &mut dyn Entity, level: &L, on_ground: bool, horizontal_collision: bool, movement: Vec3) {
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
        if (offset <= 0.5 && is_fence) || block.is_in_tag("walls") || block.data().classes.iter().any(|c| *c == "FenceGateBlock") {
            support
        } else {
            BlockPos::new(support.x(), mth::floor_f64(pos.y - f64::from(offset)), support.z())
        }
    } else {
        BlockPos::new(mth::floor_f64(pos.x), mth::floor_f64(pos.y - f64::from(offset)), mth::floor_f64(pos.z))
    }
}

/// `getOnPosLegacy()`.
pub fn get_on_pos_legacy<L: MovementLevel>(entity: &dyn Entity, level: &L) -> BlockPos {
    get_on_pos(entity, level, 0.2)
}

/// `getBlockPosBelowThatAffectsMyMovement()`.
pub fn get_block_pos_below_that_affects_my_movement<L: MovementLevel>(entity: &dyn Entity, level: &L) -> BlockPos {
    get_on_pos(entity, level, 0.500001)
}

/// `getBlockSpeedFactor()`.
pub fn get_block_speed_factor<L: MovementLevel>(entity: &dyn Entity, level: &L) -> f32 {
    let state = level.get_block_state(entity.base().block_position());
    let here = state.block().data().speed_factor;
    if state.block() != Block::WATER && state.block() != Block::BUBBLE_COLUMN {
        if here == 1.0 { level.get_block_state(get_block_pos_below_that_affects_my_movement(entity, level)).block().data().speed_factor } else { here }
    } else {
        here
    }
}

/// `getBlockJumpFactor()`.
pub fn get_block_jump_factor<L: MovementLevel>(entity: &dyn Entity, level: &L) -> f32 {
    let here = level.get_block_state(entity.base().block_position()).block().data().jump_factor;
    let below = level.get_block_state(get_block_pos_below_that_affects_my_movement(entity, level)).block().data().jump_factor;
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
        if xa.abs() <= eps { Vec3::ZERO } else { Vec3::new(xa, 0.0, 0.0) }
    } else if vec.y != 0.0 {
        let ya = restrict(Axis::Y, vec.y);
        if ya.abs() <= eps { Vec3::ZERO } else { Vec3::new(0.0, ya, 0.0) }
    } else if vec.z != 0.0 {
        let za = restrict(Axis::Z, vec.z);
        if za.abs() <= eps { Vec3::ZERO } else { Vec3::new(0.0, 0.0, za) }
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
        q.push_front(Movement { from: first.from, to: second.to, axis_dependent_original_movement: None });
    }
    q.push_back(movement);
}

/// `recordMovement(moverType, movement)`.
fn record_movement(entity: &mut dyn Entity, mover: MoverType, movement: Vec3) {
    if mover.is_server_and_client_simulated() || entity.move_simulation_type_is_server_and_client() {
        entity.base_mut().interpolation_tracker.apply_predicted_movement(movement);
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

/// `Entity.checkFallDamage(ya, onGround, onState, pos)`; returns `true` when `fallOn` has to be dispatched
/// (the block hook needs `&mut ServerLevel`, so the caller does it).
pub fn check_fall_damage(entity: &mut dyn Entity, ya: f64, on_ground: bool) -> Option<f64> {
    let b = entity.base_mut();
    if !b.was_touching_water && ya < 0.0 {
        b.fall_distance -= f64::from(ya as f32);
    }
    if on_ground {
        let fall_distance = b.fall_distance;
        b.fall_distance = 0.0;
        if fall_distance > 0.0 {
            return Some(fall_distance);
        }
    }
    None
}

/// `Entity.move(moverType, delta)`.
///
/// Returns the block position/state/fall distance the caller must run `Block.fallOn` for (hit-ground) when the
/// entity landed with a positive fall distance.
pub fn move_entity<L: MovementLevel>(entity: &mut dyn Entity, level: &L, mover: MoverType, mut delta: Vec3) -> MoveOutcome {
    let mut outcome = MoveOutcome::default();
    if entity.base().no_physics {
        let p = entity.base().position();
        entity.set_pos(p.x + delta.x, p.y + delta.y, p.z + delta.z);
        record_movement(entity, mover, delta);
        let b = entity.base_mut();
        b.horizontal_collision = false;
        b.vertical_collision = false;
        b.vertical_collision_below = false;
        b.minor_horizontal_collision = false;
        return outcome;
    }
    if mover == MoverType::Piston {
        delta = limit_piston_movement(entity, level, delta);
        if delta == Vec3::ZERO {
            return outcome;
        }
    }
    if entity.base().stuck_speed_multiplier.length_sqr() > 1.0E-7 {
        if mover != MoverType::Piston {
            delta = delta.multiply(entity.base().stuck_speed_multiplier);
        }
        entity.base_mut().stuck_speed_multiplier = Vec3::ZERO;
        entity.base_mut().set_delta_movement(Vec3::ZERO);
    }
    delta = maybe_back_off_from_edge(entity, level, delta, mover);
    let movement = collide(entity, level, delta);
    let movement_length = movement.length_sqr();
    if movement_length > 1.0E-7 || delta.length_sqr() - movement_length < 1.0E-7 {
        if entity.base().fall_distance != 0.0 && movement_length >= 1.0 {
            let check_distance = movement.length().min(8.0);
            let from = entity.base().position();
            let check_to = from.add(movement.normalize().scale(check_distance));
            let hit = level.clip(&ClipContext::new(from, check_to, ClipBlock::FalldamageResetting, ClipFluid::Water, collision_context(entity)));
            if !hit.miss {
                entity.base_mut().fall_distance = 0.0;
            }
        }
        let pos = entity.base().position();
        let new_position = pos.add(movement);
        add_movement_this_tick(entity, Movement { from: pos, to: new_position, axis_dependent_original_movement: Some(delta) });
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
        set_on_ground_with_movement(entity, level, below, horizontal_collision, movement);
    }
    let minor = horizontal_collision && entity.is_horizontal_collision_minor(movement);
    entity.base_mut().minor_horizontal_collision = minor;

    let effect_pos = get_on_pos_legacy(entity, level);
    let effect_state = level.get_block_state(effect_pos);
    if entity.is_local_instance_authoritative() {
        let on_ground = entity.base().on_ground();
        if let Some(fall) = check_fall_damage(entity, movement.y, on_ground) {
            outcome.fall_on = Some((effect_pos, effect_state, fall));
        }
    }
    if entity.is_removed() {
        return outcome;
    }
    if entity.can_simulate_movement() && ((moved_vertically && entity.base().vertical_collision) || horizontal_collision) {
        restitute_movement_after_collisions(entity, level, effect_state, x_collision, z_collision, movement, &mut outcome);
    }
    // movement emission: distance bookkeeping only (sounds / game events are dispatched by the level layer)
    let emission = entity.get_movement_emission();
    if emission.emits_anything() && !entity.base().is_passenger() {
        let moved_distance = (movement.length() * f64::from(0.6f32)) as f32;
        let horizontal_moved = (movement.horizontal_distance() * f64::from(0.6f32)) as f32;
        let supporting_pos = get_on_pos(entity, level, 1.0E-5);
        let supporting_state = level.get_block_state(supporting_pos);
        let climbing = entity.is_state_climbable(supporting_state);
        let b = entity.base_mut();
        b.move_dist += if climbing { moved_distance } else { horizontal_moved };
        b.fly_dist += moved_distance;
        if b.move_dist > b.next_step && !supporting_state.is_air() {
            outcome.step_effect_pos = Some((effect_pos, effect_state, supporting_pos, supporting_state));
            b.next_step = (b.move_dist as i32 + 1) as f32;
        }
    }
    let factor = f64::from(get_block_speed_factor(entity, level));
    let dm = entity.base().delta_movement();
    entity.base_mut().set_delta_movement(dm.multiply_xyz(factor, 1.0, factor));
    outcome
}

/// What `Entity.move` leaves for the level layer (block hooks that need `&mut ServerLevel`).
#[derive(Default, Debug)]
pub struct MoveOutcome {
    /// `Block.fallOn(level, state, pos, entity, fallDistance)` + `GameEvent.HIT_GROUND`.
    pub fall_on: Option<(BlockPos, BlockState, f64)>,
    /// `Block.bounceOn(...)` was triggered on this block (`BOUNCE` game event, `syncPosition`).
    pub bounced_on: Option<(BlockPos, BlockState)>,
    /// Step sound/vibration candidates: effect pos+state, supporting pos+state.
    pub step_effect_pos: Option<(BlockPos, BlockState, BlockPos, BlockState)>,
}

/// `restituteMovementAfterCollisions`.
fn restitute_movement_after_collisions<L: MovementLevel>(
    entity: &mut dyn Entity,
    level: &L,
    effect_state: BlockState,
    x_collision: bool,
    z_collision: bool,
    movement: Vec3,
    outcome: &mut MoveOutcome,
) {
    let suppressing = entity.is_suppressing_bounce();
    let mut restitution = if suppressing { 0.0 } else { entity.get_entity_bounciness() };
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
            restitution = if !(-current.y <= gravity) && !suppressing && !effect_state.block().is_in_tag("suppresses_bounce") {
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
        after = after.with(Axis::Y, (gravity_compensation - current.y) * effective_drag * restitution);
    }
    if bounced {
        let below = get_block_pos_below_that_affects_my_movement(entity, level);
        outcome.bounced_on = Some((below, effect_state));
        entity.base_mut().sync_position = true;
    }
    entity.base_mut().set_delta_movement(after);
}

fn block_bounciness(entity: &dyn Entity, block: Block) -> f64 {
    let mut b = block.data().bounce_restitution;
    if entity.living().is_none() {
        b *= 0.8;
    }
    f64::from(b)
}

/// `Player.maybeBackOffFromEdge` (entities without edge protection return `delta` unchanged).
fn maybe_back_off_from_edge<L: MovementLevel>(entity: &dyn Entity, level: &L, delta: Vec3, mover: MoverType) -> Vec3 {
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
    while delta_x != 0.0 && can_fall_at_least(entity, level, delta_x, 0.0, f64::from(max_down_step)) {
        if delta_x.abs() <= 0.05 {
            delta_x = 0.0;
            break;
        }
        delta_x -= step_x;
    }
    while delta_z != 0.0 && can_fall_at_least(entity, level, 0.0, delta_z, f64::from(max_down_step)) {
        if delta_z.abs() <= 0.05 {
            delta_z = 0.0;
            break;
        }
        delta_z -= step_z;
    }
    while delta_x != 0.0 && delta_z != 0.0 && can_fall_at_least(entity, level, delta_x, delta_z, f64::from(max_down_step)) {
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
    b.on_ground() || (b.fall_distance < f64::from(max_down_step) && !can_fall_at_least(entity, level, 0.0, 0.0, f64::from(max_down_step) - b.fall_distance))
}

fn can_fall_at_least<L: MovementLevel>(entity: &dyn Entity, level: &L, delta_x: f64, delta_z: f64, min_height: f64) -> bool {
    let bb = entity.base().bounding_box();
    let area = AABB::new(bb.min_x + 1.0E-7 + delta_x, bb.min_y - min_height - 1.0E-7, bb.min_z + 1.0E-7 + delta_z, bb.max_x - 1.0E-7 + delta_x, bb.min_y, bb.max_z - 1.0E-7 + delta_z);
    level.no_collision(Some(entity.id()), Some(&collision_context(entity)), &area)
}

trait SignumJava {
    fn signum_java(self) -> f64;
}

impl SignumJava for f64 {
    /// `Math.signum(double)`: 0.0/-0.0/NaN unchanged.
    fn signum_java(self) -> f64 {
        if self == 0.0 || self.is_nan() { self } else { self.signum() }
    }
}

/// `EntityFluidInteraction.update` without currents: heights above the entity's feet and whether the eyes are inside
/// water/lava. Returns whether the box touches any fluid.
pub fn update_fluid_interaction<L: MovementLevel>(entity: &mut dyn Entity, level: &L) -> bool {
    use crate::world::level::material::FluidStateExt;
    let b = entity.base_mut();
    b.fluid_height = [0.0; 2];
    b.eye_in_fluid = [false; 2];
    let Some(bx) = entity.fluid_interaction_box() else { return false };
    let (x0, y0, z0) = (mth::floor_f64(bx.min_x), mth::floor_f64(bx.min_y), mth::floor_f64(bx.min_z));
    let (x1, y1, z1) = (mth::ceil_f64(bx.max_x) - 1, mth::ceil_f64(bx.max_y) - 1, mth::ceil_f64(bx.max_z) - 1);
    let b = entity.base();
    let (entity_y, eye_x, eye_z, eye_y) = (b.bounding_box().min_y, b.get_block_x(), b.get_block_z(), b.get_eye_y());
    let mut any = false;
    let mut heights = [0.0f64; 2];
    let mut eyes = [false; 2];
    for x in x0..=x1 {
        for y in y0..=y1 {
            for z in z0..=z1 {
                let pos = BlockPos::new(x, y, z);
                let fluid = level.get_fluid_state(pos);
                if fluid.is_empty() {
                    continue;
                }
                let bottom = f64::from(y);
                let top = bottom + f64::from(fluid.get_height(level, pos));
                if top < bx.min_y {
                    continue;
                }
                any = true;
                let kind = if fluid.is_tag("water") { Some(0) } else if fluid.is_tag("lava") { Some(1) } else { None };
                let Some(k) = kind else { continue };
                if x == eye_x && z == eye_z && eye_y >= bottom && eye_y <= top {
                    eyes[k] = true;
                }
                heights[k] = heights[k].max(top - entity_y);
            }
        }
    }
    let b = entity.base_mut();
    b.fluid_height = heights;
    b.eye_in_fluid = eyes;
    any
}

/// Level-dependent part of `Entity.baseTick()`: fluid interaction, fire, lava, fall distance, shared fire flag.
pub fn base_tick_world<L: MovementLevel>(entity: &mut dyn Entity, level: &L) {
    entity.base_mut().was_eye_in_water = entity.base().eye_in_fluid[0];
    update_fluid_interaction(entity, level);
    let in_water = entity.base().fluid_height[0] > 0.0;
    if in_water {
        entity.base_mut().fall_distance = 0.0;
    }
    entity.base_mut().was_touching_water = in_water;
    let in_lava = !entity.base().first_tick && entity.base().fluid_height[1] > 0.0;
    if entity.base().remaining_fire_ticks > 0 {
        if entity.fire_immune() {
            entity.clear_fire();
        } else {
            let t = entity.base().remaining_fire_ticks;
            // `hurtServer(onFire, 1.0)` every 20 ticks when not in lava is dispatched by the damage layer
            let _ = in_lava;
            entity.set_remaining_fire_ticks(t - 1);
        }
    } else {
        entity.clear_fire();
    }
    if in_lava {
        entity.base_mut().fall_distance *= 0.5;
    }
    let burning = entity.base().remaining_fire_ticks > 0;
    entity.set_shared_flag_on_fire(burning);
    entity.base_mut().first_tick = false;
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
        // moving diagonally into a wall on +x: y first, then x (|y| >= |z|), x is clipped, z passes
        let bb = AABB::new(0.0, 0.0, 0.0, 0.6, 1.8, 0.6);
        let wall = Shapes::create(AABB::new(1.0, 0.0, -1.0, 2.0, 2.0, 2.0));
        let r = collide_with_shapes(Vec3::new(1.0, 0.0, 0.5), &bb, &[wall]);
        assert!((r.x - 0.4).abs() < 1e-9 && r.z == 0.5);
    }

    #[test]
    fn piston_axis_step_order() {
        assert_eq!(axis_step_order(Vec3::new(1.0, 0.1, 5.0)), [Axis::Y, Axis::Z, Axis::X]);
        assert_eq!(axis_step_order(Vec3::new(1.0, 5.0, 0.1)), [Axis::Y, Axis::X, Axis::Z]);
    }
}
