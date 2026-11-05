//! `CollisionContext`, `ClipContext`, ray traversal (`BlockGetter.clip` / `isBlockInLine` / `traverseBlocks`) and
//! the block part of `CollisionGetter` (`BlockCollisions`, `noCollision`, `findSupportingBlock`, `findFreePosition`).
//!
//! Entity-side collision queries (`getEntityCollisions`, entity part of `noCollision`) are implemented by the entity
//! storage on top of [`CollisionGetter::get_entity_collisions`].

use super::block::BlockStateExt;
use super::BlockGetter;
use super::material::FluidStateExt;
use crate::world::entity::EntityId;
use mc_core::direction::{Axis, Direction};
use mc_core::math::mth;
use mc_core::phys::shapes::{BooleanOp, Shapes, VoxelShape};
use mc_core::phys::{AABB, BlockHitResult, Vec3};
use mc_core::pos::{BlockPos, Cursor3D, Vec3i};
use mc_data::block::{Block, BlockState};
use mc_data::fluid::{Fluid, FluidState};
use mc_data::Item;
use std::collections::HashSet;

/// What the entity side of a [`CollisionContext`] answers (`LivingEntity.canStandOnFluid`).
pub type CanStandOnFluid = fn(Fluid) -> bool;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ContextKind {
    /// `EntityCollisionContext.Empty`: `isAbove` returns the default.
    Empty,
    Entity,
    Position,
}

/// `CollisionContext` (`EntityCollisionContext`, its `Empty`, `PositionCollisionContext` and the minecart variant's
/// ignored positions collapsed into one value type; entities build theirs with [`CollisionContext::of_entity`]).
#[derive(Clone, Debug)]
pub struct CollisionContext {
    kind: ContextKind,
    descending: bool,
    placement: bool,
    entity_bottom: f64,
    held_item: Option<Item>,
    always_collide_with_fluid: bool,
    entity: Option<EntityId>,
    can_stand_on_fluid: Option<CanStandOnFluid>,
    /// `MinecartCollisionContext.ingoreBelow` / `slopeIgnore`.
    ignored_positions: [Option<BlockPos>; 2],
    entity_fall_distance: f64,
    entity_is_falling_block: bool,
    entity_can_walk_on_powder_snow: bool,
}

impl CollisionContext {
    fn base(kind: ContextKind) -> Self {
        Self {
            kind,
            descending: false,
            placement: false,
            entity_bottom: -f64::MAX,
            held_item: None,
            always_collide_with_fluid: false,
            entity: None,
            can_stand_on_fluid: None,
            ignored_positions: [None, None],
            entity_fall_distance: 0.0,
            entity_is_falling_block: false,
            entity_can_walk_on_powder_snow: false,
        }
    }

    /// `CollisionContext.empty()`.
    pub fn empty() -> Self {
        Self::base(ContextKind::Empty)
    }

    /// `CollisionContext.emptyWithFluidCollisions()`.
    pub fn empty_with_fluid_collisions() -> Self {
        Self { always_collide_with_fluid: true, ..Self::base(ContextKind::Empty) }
    }

    /// `new EntityCollisionContext(entity, alwaysCollideWithFluid, placement)` from the entity's observable state.
    pub fn of_entity(
        entity: EntityId,
        descending: bool,
        entity_y: f64,
        held_item: Option<Item>,
        can_stand_on_fluid: Option<CanStandOnFluid>,
        always_collide_with_fluid: bool,
    ) -> Self {
        Self {
            descending,
            entity_bottom: entity_y,
            held_item,
            always_collide_with_fluid,
            entity: Some(entity),
            can_stand_on_fluid,
            ..Self::base(ContextKind::Entity)
        }
    }

    /// `CollisionContext.positionContext(y)`.
    pub fn position_context(y: f64) -> Self {
        Self { entity_bottom: y, ..Self::base(ContextKind::Position) }
    }

    /// `CollisionContext.placementContext(player)` / `withPosition(entity, y)` for an entity that is present.
    pub fn with_position(
        entity: EntityId,
        descending: bool,
        position_y: f64,
        held_item: Option<Item>,
        can_stand_on_fluid: Option<CanStandOnFluid>,
    ) -> Self {
        Self {
            descending,
            placement: true,
            entity_bottom: position_y,
            held_item,
            entity: Some(entity),
            can_stand_on_fluid,
            ..Self::base(ContextKind::Entity)
        }
    }

    /// `CollisionContext.placementContext(null)` / `withPosition(null, ..)`.
    pub fn placement_without_entity() -> Self {
        Self { placement: true, ..Self::base(ContextKind::Entity) }
    }

    /// Minecart contexts ignore the block below the rail and the upper slope block.
    pub fn with_ignored_positions(mut self, below: Option<BlockPos>, slope: Option<BlockPos>) -> Self {
        self.ignored_positions = [below, slope];
        self
    }

    pub fn is_descending(&self) -> bool {
        self.descending
    }

    /// `isAbove(shape, pos, defaultValue)`.
    pub fn is_above(&self, shape: &VoxelShape, pos: BlockPos, default_value: bool) -> bool {
        match self.kind {
            ContextKind::Empty => default_value,
            ContextKind::Entity | ContextKind::Position => {
                self.entity_bottom > f64::from(pos.y()) + shape.max(Axis::Y) - f64::from(1.0E-5f32)
            }
        }
    }

    pub fn is_holding_item(&self, item: Item) -> bool {
        self.held_item == Some(item)
    }

    pub fn always_collide_with_fluid(&self) -> bool {
        self.always_collide_with_fluid
    }

    /// `canStandOnFluid(fluidStateAbove, fluid)`.
    pub fn can_stand_on_fluid(&self, fluid_state_above: FluidState, fluid: FluidState) -> bool {
        match (self.kind, self.can_stand_on_fluid) {
            (ContextKind::Entity, Some(f)) => f(fluid.fluid()) && fluid_state_above.fluid() != fluid.fluid(),
            _ => false,
        }
    }

    pub fn is_placement(&self) -> bool {
        self.placement
    }

    /// `getEntity()`.
    pub fn entity(&self) -> Option<EntityId> {
        self.entity
    }

    /// `entity.fallDistance` of the context's entity (`PowderSnowBlock.getCollisionShape`).
    pub fn entity_fall_distance(&self) -> f64 {
        self.entity_fall_distance
    }

    /// `entity instanceof FallingBlockEntity` (`PowderSnowBlock.getCollisionShape`).
    pub fn entity_is_falling_block(&self) -> bool {
        self.entity_is_falling_block
    }

    /// `PowderSnowBlock.canEntityWalkOnPowderSnow(entity)`.
    pub fn entity_can_walk_on_powder_snow(&self) -> bool {
        self.entity_can_walk_on_powder_snow
    }

    /// Entity state read by block collision shapes: fall distance, falling block, powder snow walker.
    pub fn with_entity_state(mut self, fall_distance: f64, is_falling_block: bool, can_walk_on_powder_snow: bool) -> Self {
        self.entity_fall_distance = fall_distance;
        self.entity_is_falling_block = is_falling_block;
        self.entity_can_walk_on_powder_snow = can_walk_on_powder_snow;
        self
    }

    /// `getCollisionShape(state, collisionGetter, pos)`.
    pub fn get_collision_shape(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape {
        if self.ignored_positions.contains(&Some(pos)) {
            return Shapes::empty();
        }
        state.get_collision_shape_ctx(level, pos, self)
    }
}

/// `BlockHitResult` factories (the mc-core struct only carries the data).
pub trait BlockHitResultExt: Sized {
    fn miss(location: Vec3, direction: Direction, pos: BlockPos) -> Self;
    fn hit(location: Vec3, direction: Direction, pos: BlockPos, inside: bool) -> Self;
    fn with_direction(&self, direction: Direction) -> Self;
    fn with_position(&self, pos: BlockPos) -> Self;
}

impl BlockHitResultExt for BlockHitResult {
    fn miss(location: Vec3, direction: Direction, block_pos: BlockPos) -> Self {
        BlockHitResult { location, direction, block_pos, miss: true, inside: false, world_border_hit: false }
    }
    fn hit(location: Vec3, direction: Direction, block_pos: BlockPos, inside: bool) -> Self {
        BlockHitResult { location, direction, block_pos, miss: false, inside, world_border_hit: false }
    }
    fn with_direction(&self, direction: Direction) -> Self {
        BlockHitResult { direction, ..*self }
    }
    fn with_position(&self, block_pos: BlockPos) -> Self {
        BlockHitResult { block_pos, ..*self }
    }
}

/// `ClipContext.Block`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClipBlock {
    Collider,
    Outline,
    Visual,
    FalldamageResetting,
}

/// `ClipContext.Fluid`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClipFluid {
    None,
    SourceOnly,
    Any,
    Water,
}

impl ClipFluid {
    pub fn can_pick(self, state: FluidState) -> bool {
        match self {
            ClipFluid::None => false,
            ClipFluid::SourceOnly => state.is_source(),
            ClipFluid::Any => !state.is_empty(),
            ClipFluid::Water => state.is_tag("water"),
        }
    }
}

/// `ClipContext`.
pub struct ClipContext {
    from: Vec3,
    to: Vec3,
    block: ClipBlock,
    fluid: ClipFluid,
    collision_context: CollisionContext,
}

impl ClipContext {
    pub fn new(from: Vec3, to: Vec3, block: ClipBlock, fluid: ClipFluid, collision_context: CollisionContext) -> Self {
        Self { from, to, block, fluid, collision_context }
    }
    pub fn get_to(&self) -> Vec3 {
        self.to
    }
    pub fn get_from(&self) -> Vec3 {
        self.from
    }
    pub fn get_block_shape(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape {
        match self.block {
            ClipBlock::Collider => state.get_collision_shape_ctx(level, pos, &self.collision_context),
            ClipBlock::Outline => state.get_shape_ctx(level, pos, &self.collision_context),
            ClipBlock::Visual => state.get_visual_shape(level, pos, &self.collision_context),
            ClipBlock::FalldamageResetting => {
                if state.block().is_in_tag("fall_damage_resetting") {
                    Shapes::block()
                } else {
                    // The player-only portal cases need the entity type and `ServerLevel` rules: handled by
                    // `BlockGetterExt::clip_with` callers through `CollisionContext::entity`.
                    Shapes::empty()
                }
            }
        }
    }
    pub fn get_fluid_shape(&self, state: FluidState, level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape {
        if self.fluid.can_pick(state) { state.get_shape(level, pos) } else { Shapes::empty() }
    }
}

/// `ClipBlockStateContext`.
pub struct ClipBlockStateContext<'a> {
    pub from: Vec3,
    pub to: Vec3,
    pub is_target_block: &'a dyn Fn(BlockState) -> bool,
}

/// `BlockGetter.traverseBlocks`.
pub fn traverse_blocks<T, C>(
    from: Vec3,
    to: Vec3,
    context: &C,
    mut consumer: impl FnMut(&C, BlockPos) -> Option<T>,
    miss_factory: impl FnOnce(&C) -> T,
) -> T {
    if from == to {
        return miss_factory(context);
    }
    let to_x = mth::lerp_f64(-1.0E-7, to.x, from.x);
    let to_y = mth::lerp_f64(-1.0E-7, to.y, from.y);
    let to_z = mth::lerp_f64(-1.0E-7, to.z, from.z);
    let from_x = mth::lerp_f64(-1.0E-7, from.x, to.x);
    let from_y = mth::lerp_f64(-1.0E-7, from.y, to.y);
    let from_z = mth::lerp_f64(-1.0E-7, from.z, to.z);
    let mut cx = mth::floor_f64(from_x);
    let mut cy = mth::floor_f64(from_y);
    let mut cz = mth::floor_f64(from_z);
    if let Some(first) = consumer(context, BlockPos::new(cx, cy, cz)) {
        return first;
    }
    let (dx, dy, dz) = (to_x - from_x, to_y - from_y, to_z - from_z);
    let (sx, sy, sz) = (mth::sign(dx), mth::sign(dy), mth::sign(dz));
    let t_dx = if sx == 0 { f64::MAX } else { f64::from(sx) / dx };
    let t_dy = if sy == 0 { f64::MAX } else { f64::from(sy) / dy };
    let t_dz = if sz == 0 { f64::MAX } else { f64::from(sz) / dz };
    let mut t_x = t_dx * if sx > 0 { 1.0 - mth::frac_f64(from_x) } else { mth::frac_f64(from_x) };
    let mut t_y = t_dy * if sy > 0 { 1.0 - mth::frac_f64(from_y) } else { mth::frac_f64(from_y) };
    let mut t_z = t_dz * if sz > 0 { 1.0 - mth::frac_f64(from_z) } else { mth::frac_f64(from_z) };
    while t_x <= 1.0 || t_y <= 1.0 || t_z <= 1.0 {
        if t_x < t_y {
            if t_x < t_z {
                cx += sx;
                t_x += t_dx;
            } else {
                cz += sz;
                t_z += t_dz;
            }
        } else if t_y < t_z {
            cy += sy;
            t_y += t_dy;
        } else {
            cz += sz;
            t_z += t_dz;
        }
        if let Some(result) = consumer(context, BlockPos::new(cx, cy, cz)) {
            return result;
        }
    }
    miss_factory(context)
}

fn miss_for(from: Vec3, to: Vec3) -> BlockHitResult {
    let delta = from.subtract(to);
    BlockHitResult::miss(to, Direction::get_approximate_nearest(delta.x, delta.y, delta.z), BlockPos::containing_position(&to))
}

/// Ray queries (`BlockGetter` default methods).
pub trait BlockGetterExt: BlockGetter {
    /// `BlockGetter.isBlockInLine`.
    fn is_block_in_line(&self, c: &ClipBlockStateContext<'_>) -> BlockHitResult
    where
        Self: Sized,
    {
        traverse_blocks(
            c.from,
            c.to,
            c,
            |context, pos| {
                let state = self.get_block_state(pos);
                let delta = context.from.subtract(context.to);
                (context.is_target_block)(state).then(|| {
                    BlockHitResult::hit(
                        context.to,
                        Direction::get_approximate_nearest(delta.x, delta.y, delta.z),
                        BlockPos::containing_position(&context.to),
                        false,
                    )
                })
            },
            |context| miss_for(context.from, context.to),
        )
    }

    /// `BlockGetter.clip`.
    fn clip(&self, c: &ClipContext) -> BlockHitResult
    where
        Self: Sized,
    {
        traverse_blocks(
            c.from,
            c.to,
            c,
            |context, pos| {
                let block_state = self.get_block_state(pos);
                let fluid_state = self.get_fluid_state(pos);
                let (from, to) = (context.from, context.to);
                let block_shape = context.get_block_shape(block_state, self, pos);
                let block_result = self.clip_with_interaction_override(from, to, pos, &block_shape, block_state);
                let fluid_shape = context.get_fluid_shape(fluid_state, self, pos);
                let liquid_result = fluid_shape.clip(from, to, pos);
                let block_d = block_result.map_or(f64::MAX, |r| from.distance_to_sqr(r.location));
                let liquid_d = liquid_result.map_or(f64::MAX, |r| from.distance_to_sqr(r.location));
                if block_d <= liquid_d { block_result } else { liquid_result }
            },
            |context| miss_for(context.from, context.to),
        )
    }

    /// `BlockGetter.clipWithInteractionOverride`.
    fn clip_with_interaction_override(
        &self,
        from: Vec3,
        to: Vec3,
        pos: BlockPos,
        block_shape: &VoxelShape,
        block_state: BlockState,
    ) -> Option<BlockHitResult>
    where
        Self: Sized,
    {
        let result = block_shape.clip(from, to, pos)?;
        if let Some(hit_override) = block_state.get_interaction_shape(self, pos).clip(from, to, pos)
            && hit_override.location.subtract(from).length_sqr() < result.location.subtract(from).length_sqr()
        {
            return Some(result.with_direction(hit_override.direction));
        }
        Some(result)
    }

    /// `BlockGetter.getBlockFloorHeight(blockShape, belowBlockShape)`.
    fn get_block_floor_height_of(block_shape: &VoxelShape, below_block_shape: impl FnOnce() -> VoxelShape) -> f64
    where
        Self: Sized,
    {
        if !block_shape.is_empty() {
            block_shape.max(Axis::Y)
        } else {
            let below_floor = below_block_shape().max(Axis::Y);
            if below_floor >= 1.0 { below_floor - 1.0 } else { f64::NEG_INFINITY }
        }
    }

    /// `BlockGetter.getBlockFloorHeight(pos)`.
    fn get_block_floor_height(&self, pos: BlockPos) -> f64
    where
        Self: Sized,
    {
        Self::get_block_floor_height_of(&self.get_block_state(pos).get_collision_shape(self, pos), || {
            let below = pos.below();
            self.get_block_state(below).get_collision_shape(self, below)
        })
    }

    /// `BlockGetter.getBlockStates(box)`.
    fn get_block_states(&self, aabb: &AABB) -> Vec<BlockState> {
        BlockPos::between_closed_aabb(aabb).map(|p| self.get_block_state(p)).collect()
    }
}

impl<T: BlockGetter + ?Sized> BlockGetterExt for T {}

/// `BlockGetter.forEachBlockIntersectedBetween` visitor.
pub trait BlockStepVisitor {
    fn visit(&mut self, pos: BlockPos, iteration: i32) -> bool;
}
impl<F: FnMut(BlockPos, i32) -> bool> BlockStepVisitor for F {
    fn visit(&mut self, pos: BlockPos, iteration: i32) -> bool {
        self(pos, iteration)
    }
}

fn get_furthest_corner(direction: Vec3) -> Vec3i {
    let x_dot = Vec3::new(1.0, 0.0, 0.0).dot(direction).abs();
    let y_dot = Vec3::new(0.0, 1.0, 0.0).dot(direction).abs();
    let z_dot = Vec3::new(0.0, 0.0, 1.0).dot(direction).abs();
    let x_sign = if direction.x >= 0.0 { 1 } else { -1 };
    let y_sign = if direction.y >= 0.0 { 1 } else { -1 };
    let z_sign = if direction.z >= 0.0 { 1 } else { -1 };
    if x_dot <= y_dot && x_dot <= z_dot {
        Vec3i::new(-x_sign, -z_sign, y_sign)
    } else if y_dot <= z_dot {
        Vec3i::new(z_sign, -y_sign, -x_sign)
    } else {
        Vec3i::new(-y_sign, x_sign, -z_sign)
    }
}

/// `BlockGetter.forEachBlockIntersectedBetween`: returns `false` when the visitor aborted.
pub fn for_each_block_intersected_between(from: Vec3, to: Vec3, aabb_at_target: &AABB, visitor: &mut dyn BlockStepVisitor) -> bool {
    let travel = to.subtract(from);
    if travel.length_sqr() < mth::square_f32(1.0E-5f32) as f64 {
        for pos in BlockPos::between_closed_aabb(aabb_at_target) {
            if !visitor.visit(pos, 0) {
                return false;
            }
        }
        return true;
    }
    let mut visited: HashSet<i64> = HashSet::new();
    let back = aabb_at_target.move_vec(travel.scale(-1.0));
    for pos in BlockPos::between_corners_in_direction_aabb(&back, travel) {
        if !visitor.visit(pos, 0) {
            return false;
        }
        visited.insert(pos.as_long());
    }
    let iterations = add_collisions_along_travel(&mut visited, travel, aabb_at_target, visitor);
    if iterations < 0 {
        return false;
    }
    for pos in BlockPos::between_corners_in_direction_aabb(aabb_at_target, travel) {
        if visited.insert(pos.as_long()) && !visitor.visit(pos, iterations + 1) {
            return false;
        }
    }
    true
}

fn add_collisions_along_travel(
    visited: &mut HashSet<i64>,
    delta_move: Vec3,
    aabb_at_target: &AABB,
    visitor: &mut dyn BlockStepVisitor,
) -> i32 {
    let (box_x, box_y, box_z) = (aabb_at_target.get_xsize(), aabb_at_target.get_ysize(), aabb_at_target.get_zsize());
    let corner_dir = get_furthest_corner(delta_move);
    let to_center = aabb_at_target.get_center();
    let to_corner = Vec3::new(
        to_center.x + box_x * 0.5 * f64::from(corner_dir.x()),
        to_center.y + box_y * 0.5 * f64::from(corner_dir.y()),
        to_center.z + box_z * 0.5 * f64::from(corner_dir.z()),
    );
    let from_corner = to_corner.subtract(delta_move);
    let mut cvx = mth::floor_f64(from_corner.x);
    let mut cvy = mth::floor_f64(from_corner.y);
    let mut cvz = mth::floor_f64(from_corner.z);
    let (sx, sy, sz) = (mth::sign(delta_move.x), mth::sign(delta_move.y), mth::sign(delta_move.z));
    let t_dx = if sx == 0 { f64::MAX } else { f64::from(sx) / delta_move.x };
    let t_dy = if sy == 0 { f64::MAX } else { f64::from(sy) / delta_move.y };
    let t_dz = if sz == 0 { f64::MAX } else { f64::from(sz) / delta_move.z };
    let mut t_x = t_dx * if sx > 0 { 1.0 - mth::frac_f64(from_corner.x) } else { mth::frac_f64(from_corner.x) };
    let mut t_y = t_dy * if sy > 0 { 1.0 - mth::frac_f64(from_corner.y) } else { mth::frac_f64(from_corner.y) };
    let mut t_z = t_dz * if sz > 0 { 1.0 - mth::frac_f64(from_corner.z) } else { mth::frac_f64(from_corner.z) };
    let mut iterations = 0;
    let eps = f64::from(1.0E-5f32);
    while t_x <= 1.0 || t_y <= 1.0 || t_z <= 1.0 {
        if t_x < t_y {
            if t_x < t_z {
                cvx += sx;
                t_x += t_dx;
            } else {
                cvz += sz;
                t_z += t_dz;
            }
        } else if t_y < t_z {
            cvy += sy;
            t_y += t_dy;
        } else {
            cvz += sz;
            t_z += t_dz;
        }
        let hit = AABB::clip_box_coords(
            f64::from(cvx),
            f64::from(cvy),
            f64::from(cvz),
            f64::from(cvx + 1),
            f64::from(cvy + 1),
            f64::from(cvz + 1),
            from_corner,
            to_corner,
        );
        if let Some(hit_point) = hit {
            iterations += 1;
            let hx = mth::clamp_f64(hit_point.x, f64::from(cvx) + eps, f64::from(cvx) + 1.0 - eps);
            let hy = mth::clamp_f64(hit_point.y, f64::from(cvy) + eps, f64::from(cvy) + 1.0 - eps);
            let hz = mth::clamp_f64(hit_point.z, f64::from(cvz) + eps, f64::from(cvz) + 1.0 - eps);
            let ox = mth::floor_f64(hx - box_x * f64::from(corner_dir.x()));
            let oy = mth::floor_f64(hy - box_y * f64::from(corner_dir.y()));
            let oz = mth::floor_f64(hz - box_z * f64::from(corner_dir.z()));
            for pos in BlockPos::between_corners_in_direction_xyz(cvx, cvy, cvz, ox, oy, oz, delta_move) {
                if visited.insert(pos.as_long()) && !visitor.visit(pos, iterations) {
                    return -1;
                }
            }
        }
    }
    iterations
}

/// `BlockCollisions<T>`: the shapes (or positions) of blocks colliding with `box`, in vanilla cursor order.
pub struct BlockCollisions<'a> {
    getter: &'a dyn BlockGetter,
    available: &'a dyn Fn(i32, i32) -> bool,
    aabb: AABB,
    context: CollisionContext,
    cursor: Cursor3D,
    entity_shape: VoxelShape,
    only_suffocating_blocks: bool,
    cached_chunk: Option<(i64, bool)>,
}

impl<'a> BlockCollisions<'a> {
    /// `new BlockCollisions(collisionGetter, context, box, onlySuffocatingBlocks, resultProvider)`; `available` is
    /// `collisionGetter.getChunkForCollisions(cx, cz) != null`.
    pub fn new(
        getter: &'a dyn BlockGetter,
        available: &'a dyn Fn(i32, i32) -> bool,
        context: CollisionContext,
        aabb: AABB,
        only_suffocating_blocks: bool,
    ) -> Self {
        let x0 = mth::floor_f64(aabb.min_x - 1.0E-7) - 1;
        let x1 = mth::floor_f64(aabb.max_x + 1.0E-7) + 1;
        let y0 = mth::floor_f64(aabb.min_y - 1.0E-7) - 1;
        let y1 = mth::floor_f64(aabb.max_y + 1.0E-7) + 1;
        let z0 = mth::floor_f64(aabb.min_z - 1.0E-7) - 1;
        let z1 = mth::floor_f64(aabb.max_z + 1.0E-7) + 1;
        Self {
            getter,
            available,
            entity_shape: Shapes::create(aabb),
            aabb,
            context,
            cursor: Cursor3D::new(x0, y0, z0, x1, y1, z1),
            only_suffocating_blocks,
            cached_chunk: None,
        }
    }

    fn chunk_available(&mut self, x: i32, z: i32) -> bool {
        let (chunk_x, chunk_z) = (x >> 4, z >> 4);
        let key = mc_core::pos::ChunkPos::new(chunk_x, chunk_z).pack();
        if let Some((cached, available)) = self.cached_chunk
            && cached == key
        {
            return available;
        }
        let available = (self.available)(chunk_x, chunk_z);
        self.cached_chunk = Some((key, available));
        available
    }
}

impl Iterator for BlockCollisions<'_> {
    /// `(pos, shape moved to pos)`.
    type Item = (BlockPos, VoxelShape);

    fn next(&mut self) -> Option<Self::Item> {
        while self.cursor.advance() {
            let (x, y, z) = (self.cursor.next_x(), self.cursor.next_y(), self.cursor.next_z());
            let face_type = self.cursor.get_next_type();
            if face_type == 3 || !self.chunk_available(x, z) {
                continue;
            }
            let pos = BlockPos::new(x, y, z);
            let state = self.getter.get_block_state(pos);
            if (!self.only_suffocating_blocks || state.is_suffocating_at(self.getter, pos))
                && (face_type != 1 || state.has_large_collision_shape())
                && (face_type != 2 || state.block() == Block::MOVING_PISTON)
            {
                let block_shape = self.context.get_collision_shape(state, self.getter, pos);
                if block_shape.ptr_eq(&Shapes::block()) {
                    if self.aabb.intersects_coords(f64::from(x), f64::from(y), f64::from(z), f64::from(x) + 1.0, f64::from(y) + 1.0, f64::from(z) + 1.0) {
                        return Some((pos, block_shape.move_(f64::from(x), f64::from(y), f64::from(z))));
                    }
                } else {
                    let shape = block_shape.move_(f64::from(x), f64::from(y), f64::from(z));
                    if !shape.is_empty() && Shapes::join_is_not_empty(&shape, &self.entity_shape, BooleanOp::AND) {
                        return Some((pos, shape));
                    }
                }
            }
        }
        None
    }
}

/// The block part of vanilla's `CollisionGetter` (a blanket extension over every [`BlockGetter`]).
pub trait CollisionGetter: BlockGetter {
    /// `getChunkForCollisions(cx, cz) != null`: the chunk is loaded far enough to be collided with.
    fn chunk_available_for_collisions(&self, _chunk_x: i32, _chunk_z: i32) -> bool {
        true
    }

    /// `getEntityCollisions(source, area)`: collision boxes of entities (the entity storage overrides this).
    fn get_entity_collisions(&self, _source: Option<EntityId>, _test_area: &AABB) -> Vec<VoxelShape> {
        Vec::new()
    }

    /// `borderCollision(source, box)`: the world border's collision shape when `box` is inside/close to it.
    fn border_collision(&self, _source: EntityId, _aabb: &AABB) -> Option<VoxelShape> {
        None
    }

    /// `isUnobstructed(source, shape)`.
    fn is_unobstructed(&self, _source: Option<EntityId>, _shape: &VoxelShape) -> bool {
        true
    }

    /// `isUnobstructed(state, pos, context)`.
    fn is_unobstructed_state(&self, state: BlockState, pos: BlockPos, context: &CollisionContext) -> bool
    where
        Self: Sized,
    {
        let shape = state.get_collision_shape_ctx(self, pos, context);
        shape.is_empty() || self.is_unobstructed(None, &shape.move_vec3i(*pos))
    }

    /// `getBlockCollisionsFromContext(context, box)`.
    fn get_block_collisions_from_context(&self, context: CollisionContext, aabb: AABB) -> Vec<VoxelShape>
    where
        Self: Sized,
    {
        let available = |cx, cz| self.chunk_available_for_collisions(cx, cz);
        BlockCollisions::new(self, &available, context, aabb, false).map(|(_, shape)| shape).collect()
    }

    /// `getBlockCollisions(source, box)` with the source's context (`None` = `CollisionContext.empty()`).
    fn get_block_collisions(&self, source: Option<&CollisionContext>, aabb: &AABB) -> Vec<VoxelShape>
    where
        Self: Sized,
    {
        self.get_block_collisions_from_context(source.cloned().unwrap_or_else(CollisionContext::empty), *aabb)
    }

    /// `getBlockAndLiquidCollisions(source, box)`; `source` must be built with `always_collide_with_fluid`.
    fn get_block_and_liquid_collisions(&self, source: Option<&CollisionContext>, aabb: &AABB) -> Vec<VoxelShape>
    where
        Self: Sized,
    {
        self.get_block_collisions_from_context(source.cloned().unwrap_or_else(CollisionContext::empty_with_fluid_collisions), *aabb)
    }

    /// `noBlockCollision(entity, aabb, alwaysCollideWithFluids)`; `context` is the entity's context (for fluids it
    /// must have been created with `always_collide_with_fluid`).
    fn no_block_collision(&self, context: Option<&CollisionContext>, aabb: &AABB) -> bool
    where
        Self: Sized,
    {
        self.get_block_collisions(context, aabb).iter().all(VoxelShape::is_empty)
    }

    /// `noEntityCollision(entity, aabb)`.
    fn no_entity_collision(&self, source: Option<EntityId>, aabb: &AABB) -> bool {
        self.get_entity_collisions(source, aabb).is_empty()
    }

    /// `noBorderCollision(entity, aabb)`.
    fn no_border_collision(&self, source: Option<EntityId>, aabb: &AABB) -> bool {
        match source {
            None => true,
            Some(entity) => match self.border_collision(entity, aabb) {
                None => true,
                Some(border) => !Shapes::join_is_not_empty(&border, &Shapes::create(*aabb), BooleanOp::AND),
            },
        }
    }

    /// `noCollision(entity, aabb, alwaysCollideWithFluids)`.
    fn no_collision(&self, source: Option<EntityId>, context: Option<&CollisionContext>, aabb: &AABB) -> bool
    where
        Self: Sized,
    {
        self.no_block_collision(context, aabb) && self.no_entity_collision(source, aabb) && self.no_border_collision(source, aabb)
    }

    /// `collidesWithSuffocatingBlock(source, box)`.
    fn collides_with_suffocating_block(&self, context: Option<&CollisionContext>, aabb: &AABB) -> bool
    where
        Self: Sized,
    {
        let available = |cx, cz| self.chunk_available_for_collisions(cx, cz);
        let ctx = context.cloned().unwrap_or_else(CollisionContext::empty);
        BlockCollisions::new(self, &available, ctx, *aabb, true).any(|(_, shape)| !shape.is_empty())
    }

    /// `findSupportingBlock(source, box)`; `source_position` is `source.position()`.
    fn find_supporting_block(&self, context: &CollisionContext, source_position: Vec3, aabb: &AABB) -> Option<BlockPos>
    where
        Self: Sized,
    {
        let available = |cx, cz| self.chunk_available_for_collisions(cx, cz);
        let mut main_support: Option<BlockPos> = None;
        let mut main_support_distance = f64::MAX;
        for (pos, _) in BlockCollisions::new(self, &available, context.clone(), *aabb, false) {
            let distance = pos.dist_to_center_sqr(&source_position);
            if distance < main_support_distance
                || (distance == main_support_distance && main_support.is_none_or(|m| m < pos))
            {
                main_support = Some(pos);
                main_support_distance = distance;
            }
        }
        main_support
    }

    /// `findFreePosition(source, allowedCenters, preferredCenter, sizeX, sizeY, sizeZ)`; `border_contains` is
    /// `worldBorder.isWithinBounds(aabb)`.
    fn find_free_position(
        &self,
        context: Option<&CollisionContext>,
        border_contains: &dyn Fn(&AABB) -> bool,
        allowed_centers: &VoxelShape,
        preferred_center: Vec3,
        size_x: f64,
        size_y: f64,
        size_z: f64,
    ) -> Option<Vec3>
    where
        Self: Sized,
    {
        if allowed_centers.is_empty() {
            return None;
        }
        let search_area = allowed_centers.bounds().inflate(size_x, size_y, size_z);
        let expanded = self
            .get_block_collisions(context, &search_area)
            .into_iter()
            .filter(|shape| border_contains(&shape.bounds()))
            .flat_map(|shape| shape.to_aabbs())
            .map(|aabb| aabb.inflate(size_x / 2.0, size_y / 2.0, size_z / 2.0))
            .map(Shapes::create)
            .fold(Shapes::empty(), |a, b| Shapes::or(&a, &b));
        let free_spots = Shapes::join(allowed_centers, &expanded, BooleanOp::ONLY_FIRST);
        free_spots.closest_point_to(preferred_center)
    }
}
