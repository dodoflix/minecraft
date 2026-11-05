//! Port of the client player physics needed to stand, walk, jump, fall, fly (creative) and swim on
//! loaded terrain: `Entity.move/collide`, `LivingEntity.aiStep/travel`, `Player.travel`, the
//! `LocalPlayer` movement packet logic. Collisions use mc-core `VoxelShape`s built from mc-data.
#![allow(clippy::neg_cmp_op_on_partial_ord)] // vanilla's `!(a < b)` differs from `a >= b` for NaN

use std::sync::{LazyLock, OnceLock};

use mc_core::direction::{Axis, Direction};
use mc_core::math::mth;
use mc_core::phys::shapes::{BitSetDiscreteVoxelShape, Coords, DiscreteVoxelShape, Shapes, VoxelShape};
use mc_core::phys::{Vec3, AABB};
use mc_data::shape::{ShapeId, ShapeKind};
use mc_data::block::{BlockState, OffsetType};
use mc_data::{Block, Fluid};

use crate::world::ClientWorld;

/// Block lookups for collision; `None` = chunk not loaded.
pub trait BlockView {
    fn block_state(&self, x: i32, y: i32, z: i32) -> Option<BlockState>;
    fn min_y(&self) -> i32;
}

impl BlockView for ClientWorld {
    fn block_state(&self, x: i32, y: i32, z: i32) -> Option<BlockState> {
        ClientWorld::block_state(self, x, y, z)
    }
    fn min_y(&self) -> i32 {
        self.min_y
    }
}

// ---------------------------------------------------------------------------------------------
// Shapes

static SHAPE_CACHE: LazyLock<Vec<OnceLock<VoxelShape>>> =
    LazyLock::new(|| (0..mc_data::shape::shape_count()).map(|_| OnceLock::new()).collect());

/// Build the vanilla `VoxelShape` for an interned mc-data shape.
pub fn voxel_shape(id: ShapeId) -> &'static VoxelShape {
    SHAPE_CACHE[id.id() as usize].get_or_init(|| {
        let d = id.data();
        let bits: Vec<u64> = d.bits.chunks(8).map(|c| {
            let mut w = [0u8; 8];
            w[..c.len()].copy_from_slice(c);
            u64::from_le_bytes(w)
        }).collect();
        let [sx, sy, sz] = d.size.map(i32::from);
        let [fx, fy, fz] = d.first.map(i32::from);
        let [lx, ly, lz] = d.last.map(i32::from);
        let discrete: DiscreteVoxelShape = BitSetDiscreteVoxelShape::from_raw(sx, sy, sz, &bits, fx, fy, fz, lx, ly, lz).into();
        match d.kind {
            ShapeKind::Cube => VoxelShape::cube(discrete),
            _ => VoxelShape::array(discrete, Coords::from(d.coords[0]), Coords::from(d.coords[1]), Coords::from(d.coords[2])),
        }
    })
}

/// `Block.getOffset` for blocks with an offset function (collision of e.g. bamboo).
fn block_offset(state: BlockState, x: i32, z: i32) -> Vec3 {
    let data = state.block().data();
    let seed = mth::get_seed_xyz(x, 0, z);
    let max_h = data.max_horizontal_offset as f64;
    let comp = |s: i64| ((s & 15) as f32 / 15.0f32) as f64 - 0.5;
    let ox = mth::clamp_f64(comp(seed) * 0.5, -max_h, max_h);
    let oz = mth::clamp_f64(comp(seed >> 8) * 0.5, -max_h, max_h);
    match data.offset_type {
        OffsetType::None => Vec3::ZERO,
        OffsetType::Xz => Vec3::new(ox, 0.0, oz),
        OffsetType::Xyz => {
            let oy = (((seed >> 4) & 15) as f32 / 15.0f32 - 1.0) as f64 * data.max_vertical_offset as f64;
            Vec3::new(ox, oy, oz)
        }
    }
}

/// Collision shape of the block at `(x, y, z)` moved into place, and whether it really overlaps `area`.
fn collision_at(state: BlockState, x: i32, y: i32, z: i32, area: &AABB) -> Option<VoxelShape> {
    let sid = state.collision_shape();
    if sid.is_empty() {
        return None;
    }
    let off = if state.data().flags & mc_data::block::state_flags::HAS_OFFSET != 0 {
        block_offset(state, x, z)
    } else {
        Vec3::ZERO
    };
    let (dx, dy, dz) = (x as f64 + off.x, y as f64 + off.y, z as f64 + off.z);
    let overlaps = sid.data().aabbs.iter().any(|b| {
        area.intersects_coords(b[0] + dx, b[1] + dy, b[2] + dz, b[3] + dx, b[4] + dy, b[5] + dz)
    });
    overlaps.then(|| voxel_shape(sid).move_(dx, dy, dz))
}

/// `Level.getBlockCollisions(entity, box)` (unloaded chunks collide with nothing).
pub fn block_collisions<W: BlockView>(world: &W, area: &AABB) -> Vec<VoxelShape> {
    let (x0, x1) = (mth::floor_f64(area.min_x - 1.0E-7) - 1, mth::floor_f64(area.max_x + 1.0E-7) + 1);
    let (y0, y1) = (mth::floor_f64(area.min_y - 1.0E-7) - 1, mth::floor_f64(area.max_y + 1.0E-7) + 1);
    let (z0, z1) = (mth::floor_f64(area.min_z - 1.0E-7) - 1, mth::floor_f64(area.max_z + 1.0E-7) + 1);
    let mut out = Vec::new();
    for x in x0..=x1 {
        for y in y0..=y1 {
            for z in z0..=z1 {
                if let Some(state) = world.block_state(x, y, z)
                    && let Some(shape) = collision_at(state, x, y, z, area)
                {
                    out.push(shape);
                }
            }
        }
    }
    out
}

/// `Level.noCollision(box)` for blocks.
pub fn no_collision<W: BlockView>(world: &W, area: &AABB) -> bool {
    block_collisions(world, area).is_empty()
}

fn axis_get(v: Vec3, axis: Axis) -> f64 {
    match axis {
        Axis::X => v.x,
        Axis::Y => v.y,
        Axis::Z => v.z,
    }
}

fn axis_with(v: Vec3, axis: Axis, value: f64) -> Vec3 {
    match axis {
        Axis::X => Vec3::new(value, v.y, v.z),
        Axis::Y => Vec3::new(v.x, value, v.z),
        Axis::Z => Vec3::new(v.x, v.y, value),
    }
}

fn collide_with_shapes(movement: Vec3, bb: &AABB, shapes: &[VoxelShape]) -> Vec3 {
    if shapes.is_empty() {
        return movement;
    }
    let mut resolved = Vec3::ZERO;
    for axis in Direction::axis_step_order(movement) {
        let m = axis_get(movement, axis);
        if m != 0.0 {
            let moved = bb.move_(resolved.x, resolved.y, resolved.z);
            resolved = axis_with(resolved, axis, Shapes::collide(axis, &moved, shapes.iter(), m));
        }
    }
    resolved
}

fn horizontal_distance_sqr(v: Vec3) -> f64 {
    v.x * v.x + v.z * v.z
}

/// `Entity.collide` with step-up (`max_up_step` = attribute `step_height`).
pub fn collide<W: BlockView>(world: &W, bb: &AABB, movement: Vec3, on_ground: bool, max_up_step: f32) -> Vec3 {
    let swept = bb.expand_towards(movement.x, movement.y, movement.z).expand_towards(0.0, max_up_step as f64, 0.0);
    let colliders = block_collisions(world, &swept);
    let step = if movement.x * movement.x + movement.y * movement.y + movement.z * movement.z == 0.0 {
        movement
    } else {
        collide_with_shapes(movement, bb, &colliders)
    };
    let (x_col, y_col, z_col) = (movement.x != step.x, movement.y != step.y, movement.z != step.z);
    let on_ground_after = y_col && movement.y < 0.0;
    if max_up_step > 0.0 && (on_ground_after || on_ground) && (x_col || z_col) {
        let grounded = if on_ground_after { bb.move_(0.0, step.y, 0.0) } else { *bb };
        let mut step_area = grounded.expand_towards(movement.x, max_up_step as f64, movement.z);
        if !on_ground_after {
            step_area = step_area.expand_towards(0.0, -(1.0E-5f32 as f64), 0.0);
        }
        let colliders = block_collisions(world, &step_area);
        let skip = step.y as f32;
        // collectCandidateStepUpHeights
        let mut candidates: Vec<f32> = Vec::new();
        for c in &colliders {
            for coord in c.get_coords(Axis::Y).to_vec() {
                let rel = (coord - grounded.min_y) as f32;
                if !(rel < 0.0) && rel != skip {
                    if rel > max_up_step {
                        break;
                    }
                    if !candidates.contains(&rel) {
                        candidates.push(rel);
                    }
                }
            }
        }
        candidates.sort_by(f32::total_cmp);
        for h in candidates {
            let from_ground = collide_with_shapes(Vec3::new(movement.x, h as f64, movement.z), &grounded, &colliders);
            if horizontal_distance_sqr(from_ground) > horizontal_distance_sqr(step) {
                let dist = bb.min_y - grounded.min_y;
                return Vec3::new(from_ground.x, from_ground.y - dist, from_ground.z);
            }
        }
    }
    step
}

// ---------------------------------------------------------------------------------------------
// Fluids

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FluidKind {
    Water,
    Lava,
}

fn fluid_kind(state: BlockState) -> Option<FluidKind> {
    let fs = state.get_fluid_state();
    if fs.data().is_empty {
        return None;
    }
    match fs.data().fluid {
        f if f == Fluid::WATER || f == Fluid::FLOWING_WATER => Some(FluidKind::Water),
        f if f == Fluid::LAVA || f == Fluid::FLOWING_LAVA => Some(FluidKind::Lava),
        _ => None,
    }
}

/// `FluidState.getHeight`: full height when the same fluid is above.
fn fluid_height_at<W: BlockView>(world: &W, x: i32, y: i32, z: i32, state: BlockState, kind: FluidKind) -> f64 {
    let above = world.block_state(x, y + 1, z).and_then(fluid_kind);
    if above == Some(kind) { 1.0 } else { state.get_fluid_state().data().own_height as f64 }
}

/// `updateFluidHeightAndDoFluidPushing` without the flow push: `(touching, height above feet)`.
pub fn fluid_height<W: BlockView>(world: &W, bb: &AABB, kind: FluidKind) -> (bool, f64) {
    let b = bb.deflate_scalar(0.001);
    let (x0, x1) = (mth::floor_f64(b.min_x), mth::ceil_f64(b.max_x));
    let (y0, y1) = (mth::floor_f64(b.min_y), mth::ceil_f64(b.max_y));
    let (z0, z1) = (mth::floor_f64(b.min_z), mth::ceil_f64(b.max_z));
    let (mut found, mut max) = (false, 0.0f64);
    for x in x0..x1 {
        for y in y0..y1 {
            for z in z0..z1 {
                let Some(state) = world.block_state(x, y, z) else { continue };
                if fluid_kind(state) != Some(kind) {
                    continue;
                }
                let top = y as f64 + fluid_height_at(world, x, y, z, state, kind);
                if top >= b.min_y {
                    found = true;
                    max = max.max(top - b.min_y);
                }
            }
        }
    }
    (found, max)
}

pub fn contains_any_liquid<W: BlockView>(world: &W, bb: &AABB) -> bool {
    let (x0, x1) = (mth::floor_f64(bb.min_x), mth::ceil_f64(bb.max_x));
    let (y0, y1) = (mth::floor_f64(bb.min_y), mth::ceil_f64(bb.max_y));
    let (z0, z1) = (mth::floor_f64(bb.min_z), mth::ceil_f64(bb.max_z));
    (x0..x1).any(|x| {
        (y0..y1).any(|y| (z0..z1).any(|z| world.block_state(x, y, z).is_some_and(|s| !s.get_fluid_state().data().is_empty)))
    })
}

// ---------------------------------------------------------------------------------------------
// Player body

pub use mc_protocol::packets::play::Input;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Abilities {
    pub invulnerable: bool,
    pub flying: bool,
    pub may_fly: bool,
    pub instabuild: bool,
    pub may_build: bool,
    pub flying_speed: f32,
    pub walking_speed: f32,
}

impl Default for Abilities {
    fn default() -> Self {
        Self { invulnerable: false, flying: false, may_fly: false, instabuild: false, may_build: true, flying_speed: 0.05, walking_speed: 0.1 }
    }
}

/// Width/height of the player hitbox per pose and eye height.
pub const WIDTH: f64 = 0.6;
pub const HEIGHT_STANDING: f64 = 1.8;
pub const HEIGHT_CROUCHING: f64 = 1.5;
pub const EYE_STANDING: f64 = 1.62;
pub const EYE_CROUCHING: f64 = 1.27;
const GRAVITY: f64 = 0.08; // attribute base 0.08
const JUMP_POWER: f32 = 0.42; // (float) attribute jump_strength 0.42
const STEP_HEIGHT: f32 = 0.6;
const SNEAK_SPEED: f64 = 0.3;

/// The movement packet the tick decided on (`LocalPlayer.sendPosition`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MoveOut {
    PosRot { x: f64, y: f64, z: f64, yaw: f32, pitch: f32, on_ground: bool, h_collision: bool },
    Pos { x: f64, y: f64, z: f64, on_ground: bool, h_collision: bool },
    Rot { yaw: f32, pitch: f32, on_ground: bool, h_collision: bool },
    Status { on_ground: bool, h_collision: bool },
}

#[derive(Debug, Clone)]
pub struct PlayerBody {
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
    pub horizontal_collision: bool,
    pub vertical_collision: bool,
    pub vertical_collision_below: bool,
    pub fall_distance: f64,
    pub sprinting: bool,
    pub crouching: bool,
    pub in_water: bool,
    pub in_lava: bool,
    pub abilities: Abilities,
    /// Jump boost amplifier+1 (0 = none) / speed effect amplifier+1 (0 = none).
    pub jump_boost: i32,
    pub speed_effect: i32,
    pub slow_effect: i32,
    pub food_level: i32,
    no_jump_delay: i32,
    jumping: bool,
    last: Option<(f64, f64, f64, f32, f32)>,
    last_on_ground: bool,
    last_h_collision: bool,
    position_reminder: i32,
}

impl PlayerBody {
    pub fn new(pos: Vec3, yaw: f32, pitch: f32) -> Self {
        Self {
            pos,
            vel: Vec3::ZERO,
            yaw,
            pitch,
            on_ground: false,
            horizontal_collision: false,
            vertical_collision: false,
            vertical_collision_below: false,
            fall_distance: 0.0,
            sprinting: false,
            crouching: false,
            in_water: false,
            in_lava: false,
            abilities: Abilities::default(),
            jump_boost: 0,
            speed_effect: 0,
            slow_effect: 0,
            food_level: 20,
            no_jump_delay: 0,
            jumping: false,
            last: Some((pos.x, pos.y, pos.z, yaw, pitch)),
            last_on_ground: false,
            last_h_collision: false,
            position_reminder: 0,
        }
    }

    pub fn height(&self) -> f64 {
        if self.crouching { HEIGHT_CROUCHING } else { HEIGHT_STANDING }
    }

    pub fn eye_height(&self) -> f64 {
        if self.crouching { EYE_CROUCHING } else { EYE_STANDING }
    }

    pub fn bounding_box(&self) -> AABB {
        self.bounding_box_at(self.pos, self.height())
    }

    fn bounding_box_at(&self, p: Vec3, h: f64) -> AABB {
        AABB::new(p.x - WIDTH / 2.0, p.y, p.z - WIDTH / 2.0, p.x + WIDTH / 2.0, p.y + h, p.z + WIDTH / 2.0)
    }

    pub fn eye_position(&self) -> Vec3 {
        Vec3::new(self.pos.x, self.pos.y + self.eye_height(), self.pos.z)
    }

    /// `Entity.getLookAngle`
    pub fn look_vector(&self) -> Vec3 {
        let (yr, xr) = (self.yaw * (std::f32::consts::PI / 180.0), self.pitch * (std::f32::consts::PI / 180.0));
        let (ycos, ysin) = (mth::cos(-yr as f64 - std::f32::consts::PI as f64), mth::sin(-yr as f64 - std::f32::consts::PI as f64));
        let (xcos, xsin) = (-mth::cos(-xr as f64), mth::sin(-xr as f64));
        Vec3::new((ysin * xcos) as f64, xsin as f64, (ycos * xcos) as f64)
    }

    /// Server-forced position (teleport): `moveTo` + stop velocity as given.
    pub fn teleport(&mut self, pos: Vec3, vel: Vec3, yaw: f32, pitch: f32) {
        self.pos = pos;
        self.vel = vel;
        self.yaw = yaw;
        self.pitch = pitch;
        self.last = Some((pos.x, pos.y, pos.z, yaw, pitch));
        self.position_reminder = 0;
    }

    fn block_pos_below_affecting_movement(&self) -> (i32, i32, i32) {
        (mth::floor_f64(self.pos.x), mth::floor_f64(self.pos.y - 0.500001f32 as f64), mth::floor_f64(self.pos.z))
    }

    fn block_speed_factor<W: BlockView>(&self, world: &W) -> f32 {
        if self.abilities.flying {
            return 1.0;
        }
        let here = world.block_state(self.block_x(), self.block_y(), self.block_z());
        let f_here = here.map_or(1.0, |s| s.block().data().speed_factor);
        if here.is_some_and(|s| s.block() == Block::WATER || s.block() == Block::BUBBLE_COLUMN) {
            return f_here;
        }
        if f_here == 1.0 {
            let (x, y, z) = self.block_pos_below_affecting_movement();
            world.block_state(x, y, z).map_or(1.0, |s| s.block().data().speed_factor)
        } else {
            f_here
        }
    }

    fn block_jump_factor<W: BlockView>(&self, world: &W) -> f32 {
        let here = world.block_state(self.block_x(), self.block_y(), self.block_z()).map_or(1.0, |s| s.block().data().jump_factor);
        if here == 1.0 {
            let (x, y, z) = self.block_pos_below_affecting_movement();
            world.block_state(x, y, z).map_or(1.0, |s| s.block().data().jump_factor)
        } else {
            here
        }
    }

    pub fn block_x(&self) -> i32 {
        mth::floor_f64(self.pos.x)
    }
    pub fn block_y(&self) -> i32 {
        mth::floor_f64(self.pos.y)
    }
    pub fn block_z(&self) -> i32 {
        mth::floor_f64(self.pos.z)
    }

    /// `onClimbable` for blocks of the `climbable` tag at the feet (no trapdoor-above-ladder special case).
    fn on_climbable<W: BlockView>(&self, world: &W) -> bool {
        if self.abilities.flying {
            return false;
        }
        world.block_state(self.block_x(), self.block_y(), self.block_z()).is_some_and(|s| s.block().is_in_tag("climbable"))
    }

    fn movement_speed(&self) -> f32 {
        let base = self.abilities.walking_speed as f64;
        let mut value = base;
        // attribute modifiers (all ADD_MULTIPLIED_TOTAL-like composition is approximated for the common cases)
        let mut mult = 1.0;
        if self.sprinting {
            mult *= 1.0 + 0.3f32 as f64;
        }
        if self.speed_effect > 0 {
            mult *= 1.0 + 0.2f32 as f64 * self.speed_effect as f64;
        }
        if self.slow_effect > 0 {
            mult *= 1.0 - 0.15f32 as f64 * self.slow_effect as f64;
        }
        value *= mult;
        value as f32
    }

    fn flying_speed(&self) -> f32 {
        if self.abilities.flying {
            if self.sprinting { self.abilities.flying_speed * 2.0 } else { self.abilities.flying_speed }
        } else if self.sprinting {
            0.025999999
        } else {
            0.02
        }
    }

    fn jump_power<W: BlockView>(&self, world: &W) -> f32 {
        let boost = if self.jump_boost > 0 { 0.1f32 * self.jump_boost as f32 } else { 0.0 };
        JUMP_POWER * 1.0 * self.block_jump_factor(world) + boost
    }

    fn jump_from_ground<W: BlockView>(&mut self, world: &W) {
        let power = self.jump_power(world);
        if power > 1.0E-5 {
            self.vel.y = (power as f64).max(self.vel.y);
            if self.sprinting {
                let a = self.yaw * (std::f32::consts::PI / 180.0);
                self.vel = Vec3::new(self.vel.x - mth::sin(a as f64) as f64 * 0.2, self.vel.y, self.vel.z + mth::cos(a as f64) as f64 * 0.2);
            }
        }
    }

    fn move_relative(&mut self, speed: f32, input: Vec3) {
        let len = input.x * input.x + input.y * input.y + input.z * input.z;
        if len < 1.0E-7 {
            return;
        }
        let m = if len > 1.0 {
            let l = len.sqrt();
            Vec3::new(input.x / l, input.y / l, input.z / l)
        } else {
            input
        };
        let m = Vec3::new(m.x * speed as f64, m.y * speed as f64, m.z * speed as f64);
        let (sin, cos) = (mth::sin((self.yaw * (std::f32::consts::PI / 180.0)) as f64), mth::cos((self.yaw * (std::f32::consts::PI / 180.0)) as f64));
        self.vel = Vec3::new(
            self.vel.x + m.x * cos as f64 - m.z * sin as f64,
            self.vel.y + m.y,
            self.vel.z + m.z * cos as f64 + m.x * sin as f64,
        );
    }

    /// `Player.maybeBackOffFromEdge` (sneaking at ledges).
    fn back_off_from_edge<W: BlockView>(&self, world: &W, delta: Vec3) -> Vec3 {
        let max_down = STEP_HEIGHT as f64;
        let staying = self.crouching && !self.abilities.flying;
        if !staying || delta.y > 0.0 {
            return delta;
        }
        let can_fall = |dx: f64, dz: f64, min_h: f64| {
            let b = self.bounding_box();
            no_collision(world, &AABB::new(b.min_x + 1.0E-7 + dx, b.min_y - min_h - 1.0E-7, b.min_z + 1.0E-7 + dz, b.max_x - 1.0E-7 + dx, b.min_y, b.max_z - 1.0E-7 + dz))
        };
        let above_ground = self.on_ground || (self.fall_distance < max_down && !can_fall(0.0, 0.0, max_down - self.fall_distance));
        if !above_ground {
            return delta;
        }
        let (mut dx, mut dz) = (delta.x, delta.z);
        let (sx, sz) = (dx.signum() * 0.05, dz.signum() * 0.05);
        while dx != 0.0 && can_fall(dx, 0.0, max_down) {
            if dx.abs() <= 0.05 {
                dx = 0.0;
                break;
            }
            dx -= sx;
        }
        while dz != 0.0 && can_fall(0.0, dz, max_down) {
            if dz.abs() <= 0.05 {
                dz = 0.0;
                break;
            }
            dz -= sz;
        }
        while dx != 0.0 && dz != 0.0 && can_fall(dx, dz, max_down) {
            dx = if dx.abs() <= 0.05 { 0.0 } else { dx - sx };
            dz = if dz.abs() <= 0.05 { 0.0 } else { dz - sz };
        }
        Vec3::new(dx, delta.y, dz)
    }

    /// `Entity.move(MoverType.SELF, delta)` for the local player.
    pub fn move_entity<W: BlockView>(&mut self, world: &W, delta: Vec3) {
        let delta = self.back_off_from_edge(world, delta);
        let bb = self.bounding_box();
        let movement = collide(world, &bb, delta, self.on_ground, STEP_HEIGHT);
        let movement_sqr = movement.x * movement.x + movement.y * movement.y + movement.z * movement.z;
        let delta_sqr = delta.x * delta.x + delta.y * delta.y + delta.z * delta.z;
        if movement_sqr > 1.0E-7 || delta_sqr - movement_sqr < 1.0E-7 {
            self.pos = Vec3::new(self.pos.x + movement.x, self.pos.y + movement.y, self.pos.z + movement.z);
        }
        let x_col = !mth::equal_f64(delta.x, movement.x);
        let z_col = !mth::equal_f64(delta.z, movement.z);
        self.horizontal_collision = x_col || z_col;
        // the local player is authoritative: always updates vertical state
        self.vertical_collision = delta.y != movement.y;
        self.vertical_collision_below = self.vertical_collision && delta.y < 0.0;
        self.on_ground = self.vertical_collision_below;

        // checkFallDamage (client side tracking only)
        if self.on_ground {
            self.fall_distance = 0.0;
        } else if movement.y < 0.0 {
            self.fall_distance -= movement.y;
        }
        if movement.y > 0.0 && self.vertical_collision && !self.on_ground {
            // head bump keeps accumulated fall distance
        }
        if self.horizontal_collision || (self.vertical_collision) {
            self.restitute(world, x_col, z_col, movement);
        }
        let f = self.block_speed_factor(world) as f64;
        self.vel = Vec3::new(self.vel.x * f, self.vel.y, self.vel.z * f);
    }

    fn restitute<W: BlockView>(&mut self, world: &W, x_col: bool, z_col: bool, movement: Vec3) {
        // restitution only from block bounciness (slime) when not sneaking
        let mut v = self.vel;
        if self.vertical_collision && self.vertical_collision_below && !self.crouching {
            let (x, y, z) = (self.block_x(), mth::floor_f64(self.pos.y - 0.2f32 as f64), self.block_z());
            let restitution = world.block_state(x, y, z).map_or(0.0, |s| s.block().data().bounce_restitution as f64);
            let suppress = world.block_state(x, y, z).is_some_and(|s| s.block().is_in_tag("suppresses_bounce"));
            if restitution > 0.0 && !(-self.vel.y <= GRAVITY) && !suppress {
                let portion = movement.y / self.vel.y;
                let gravity_comp = portion * GRAVITY;
                let drag = 1.0 + (0.98f32 as f64 - 1.0) * portion;
                v.y = (gravity_comp - self.vel.y) * drag * restitution;
                self.vel = v;
            }
        }
        let _ = (x_col, z_col);
    }

    /// One client tick of `LocalPlayer.aiStep` + `LivingEntity.travel`. Returns nothing; use
    /// [`Self::position_packet`] afterwards.
    pub fn tick<W: BlockView>(&mut self, world: &W, input: Input) {
        // `LocalPlayer.aiStep`: input → impulses
        let mut forward = (input.forward as i32 - input.backward as i32) as f64;
        let mut left = (input.left as i32 - input.right as i32) as f64;
        self.crouching = input.shift && !self.abilities.flying;
        if input.shift {
            forward *= SNEAK_SPEED;
            left *= SNEAK_SPEED;
        }
        self.update_sprint(&input, forward);
        self.jumping = input.jump;
        let mut vertical = 0.0;
        if self.abilities.flying {
            // creative flight: shift descends, jump ascends
            let ya = input.jump as i32 - input.shift as i32;
            if ya != 0 {
                vertical = ya as f64 * self.abilities.flying_speed as f64 * 3.0;
            }
        }
        // 0.98 `applyInput`
        let (xxa, zza) = ((left as f32 * 0.98f32) as f64, (forward as f32 * 0.98f32) as f64);

        // LivingEntity.aiStep
        if self.no_jump_delay > 0 {
            self.no_jump_delay -= 1;
        }
        let (mut dx, mut dy, mut dz) = (self.vel.x, self.vel.y, self.vel.z);
        if horizontal_distance_sqr(self.vel) < 9.0E-6 {
            dx = 0.0;
            dz = 0.0;
        }
        if dy.abs() < 0.003 {
            dy = 0.0;
        }
        self.vel = Vec3::new(dx, dy, dz);
        if vertical != 0.0 {
            self.vel.y += vertical;
        }

        let bb = self.bounding_box();
        let (touch_w, fh_w) = fluid_height(world, &bb, FluidKind::Water);
        let (touch_l, fh_l) = fluid_height(world, &bb, FluidKind::Lava);
        let affected_by_fluids = !self.abilities.flying;
        self.in_water = touch_w;
        self.in_lava = touch_l && !touch_w || (touch_l && fh_l > 0.0);
        let fluid_jump_threshold = if self.eye_height() < 0.4 { 0.0 } else { 0.4 };
        if self.jumping && affected_by_fluids {
            let fluid_h = if self.in_lava { fluid_height(world, &bb, FluidKind::Lava).1 } else { fh_w };
            let in_water_and_h = self.in_water && fluid_h > 0.0;
            if !in_water_and_h || self.on_ground && !(fluid_h > fluid_jump_threshold) {
                if !self.in_lava || self.on_ground && fluid_h <= fluid_jump_threshold {
                    if (self.on_ground || in_water_and_h && fluid_h <= fluid_jump_threshold) && self.no_jump_delay == 0 {
                        self.jump_from_ground(world);
                        self.no_jump_delay = 10;
                    }
                } else {
                    self.vel.y += 0.04f32 as f64;
                }
            } else {
                self.vel.y += 0.04f32 as f64;
            }
        } else {
            self.no_jump_delay = 0;
        }

        // Player.travel
        let in_vec = Vec3::new(xxa, 0.0, zza);
        if self.abilities.flying {
            let original_y = self.vel.y;
            self.travel(world, in_vec, fh_w, fh_l);
            self.vel.y = original_y * 0.6;
        } else {
            self.travel(world, in_vec, fh_w, fh_l);
        }

        // LocalPlayer: landing cancels flight
        if self.on_ground && self.abilities.flying {
            self.abilities.flying = false;
        }
    }

    fn update_sprint(&mut self, input: &Input, forward_impulse: f64) {
        let has_forward = forward_impulse > 1.0E-5;
        let can_sprint = (self.food_level as f32 > 6.0 || self.abilities.may_fly) && !input.shift;
        if self.sprinting {
            if !has_forward || self.horizontal_collision || !can_sprint || self.in_water && !self.abilities.flying {
                self.sprinting = false;
            }
        } else if input.sprint && has_forward && can_sprint && !self.in_water {
            self.sprinting = true;
        }
    }

    fn travel<W: BlockView>(&mut self, world: &W, input: Vec3, fh_water: f64, _fh_lava: f64) {
        if (self.in_water || self.in_lava) && !self.abilities.flying {
            self.travel_in_fluid(world, input, fh_water);
        } else {
            self.travel_in_air(world, input);
        }
    }

    fn travel_in_air<W: BlockView>(&mut self, world: &W, input: Vec3) {
        let (bx, by, bz) = self.block_pos_below_affecting_movement();
        let block_friction: f32 = if self.on_ground { world.block_state(bx, by, bz).map_or(0.6, |s| s.block().data().friction) } else { 1.0 };
        // handleRelativeFrictionAndCalculateMovement
        let speed = if self.on_ground {
            if block_friction > 0.6 { self.movement_speed() * (0.21600002f32 / (block_friction * block_friction * block_friction)) } else { self.movement_speed() }
        } else {
            self.flying_speed()
        };
        self.move_relative(speed, input);
        if self.on_climbable(world) {
            self.fall_distance = 0.0;
            let (xd, zd) = (self.vel.x.clamp(-0.15f32 as f64, 0.15f32 as f64), self.vel.z.clamp(-0.15f32 as f64, 0.15f32 as f64));
            let mut yd = self.vel.y.max(-0.15f32 as f64);
            if yd < 0.0 && self.crouching {
                yd = 0.0;
            }
            self.vel = Vec3::new(xd, yd, zd);
        }
        self.move_entity(world, self.vel);
        let mut movement = self.vel;
        if (self.horizontal_collision || self.jumping) && self.on_climbable(world) {
            movement = Vec3::new(movement.x, 0.2, movement.z);
        }
        let mut movement_y = movement.y;
        let (px, py, pz) = self.block_pos_below_affecting_movement();
        if world.block_state(px, py, pz).is_some() {
            movement_y -= GRAVITY;
        } else if self.pos.y > world.min_y() as f64 {
            movement_y = -0.1;
        } else {
            movement_y = 0.0;
        }
        let friction = block_friction * 0.91f32;
        self.vel = Vec3::new(movement.x * friction as f64, movement_y * 0.98f32 as f64, movement.z * friction as f64);
    }

    fn travel_in_fluid<W: BlockView>(&mut self, world: &W, input: Vec3, _fh: f64) {
        let is_falling = self.vel.y <= 0.0;
        let old_y = self.pos.y;
        if self.in_water {
            let slow_down: f32 = if self.sprinting { 0.9 } else { 0.8 };
            self.move_relative(0.02, input);
            self.move_entity(world, self.vel);
            let mut movement = self.vel;
            if self.horizontal_collision && self.on_climbable(world) {
                movement.y = 0.2;
            }
            movement = Vec3::new(movement.x * slow_down as f64, movement.y * 0.8f32 as f64, movement.z * slow_down as f64);
            self.vel = self.fluid_falling_adjusted(GRAVITY, is_falling, movement);
        } else {
            self.move_relative(0.02, input);
            self.move_entity(world, self.vel);
            let shallow = fluid_height(world, &self.bounding_box(), FluidKind::Lava).1 <= if self.eye_height() < 0.4 { 0.0 } else { 0.4 };
            if shallow {
                self.vel = Vec3::new(self.vel.x * 0.5, self.vel.y * 0.8f32 as f64, self.vel.z * 0.5);
                self.vel = self.fluid_falling_adjusted(GRAVITY, is_falling, self.vel);
            } else {
                self.vel = Vec3::new(self.vel.x * 0.5, self.vel.y * 0.5, self.vel.z * 0.5);
            }
            self.vel.y += -GRAVITY / 4.0;
        }
        // jumpOutOfFluid
        if self.horizontal_collision {
            let b = self.bounding_box().move_(self.vel.x, self.vel.y + 0.6f32 as f64 - self.pos.y + old_y, self.vel.z);
            if no_collision(world, &b) && !contains_any_liquid(world, &b) {
                self.vel.y = 0.3f32 as f64;
            }
        }
    }

    fn fluid_falling_adjusted(&self, base_gravity: f64, is_falling: bool, movement: Vec3) -> Vec3 {
        if base_gravity != 0.0 && !self.sprinting {
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

    /// `LocalPlayer.sendPosition`: the movement packet to send after this tick (None = nothing changed and
    /// the on-ground/collision flags are unchanged).
    pub fn position_packet(&mut self) -> Option<MoveOut> {
        let (lx, ly, lz, lyaw, lpitch) = self.last.unwrap_or((self.pos.x, self.pos.y, self.pos.z, self.yaw, self.pitch));
        let (dx, dy, dz) = (self.pos.x - lx, self.pos.y - ly, self.pos.z - lz);
        self.position_reminder += 1;
        let moved = dx * dx + dy * dy + dz * dz > (2.0E-4f64) * (2.0E-4f64) || self.position_reminder >= 20;
        let rotated = self.yaw != lyaw || self.pitch != lpitch;
        let (g, h) = (self.on_ground, self.horizontal_collision);
        let out = if moved && rotated {
            Some(MoveOut::PosRot { x: self.pos.x, y: self.pos.y, z: self.pos.z, yaw: self.yaw, pitch: self.pitch, on_ground: g, h_collision: h })
        } else if moved {
            Some(MoveOut::Pos { x: self.pos.x, y: self.pos.y, z: self.pos.z, on_ground: g, h_collision: h })
        } else if rotated {
            Some(MoveOut::Rot { yaw: self.yaw, pitch: self.pitch, on_ground: g, h_collision: h })
        } else if self.last_on_ground != g || self.last_h_collision != h {
            Some(MoveOut::Status { on_ground: g, h_collision: h })
        } else {
            None
        };
        if moved {
            self.last = Some((self.pos.x, self.pos.y, self.pos.z, lyaw, lpitch));
            self.position_reminder = 0;
        }
        if rotated {
            let (x, y, z, _, _) = self.last.expect("set");
            self.last = Some((x, y, z, self.yaw, self.pitch));
        }
        self.last_on_ground = g;
        self.last_h_collision = h;
        out
    }

    /// Record that a packet carrying the full position was sent (teleport confirmation reply).
    pub fn mark_sent(&mut self) {
        self.last = Some((self.pos.x, self.pos.y, self.pos.z, self.yaw, self.pitch));
        self.position_reminder = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stone() -> u16 {
        Block::STONE.default_state().id() as u16
    }

    /// 3x3 chunks of air with a stone floor filling y < 64.
    fn flat_world() -> ClientWorld {
        let mut w = ClientWorld::new(0, 128, 64);
        for cx in -1..=1 {
            for cz in -1..=1 {
                w.insert_air_chunk(cx, cz);
                for x in 0..16 {
                    for z in 0..16 {
                        for y in 0..64 {
                            w.set_block_state_id(cx * 16 + x, y, cz * 16 + z, stone());
                        }
                    }
                }
            }
        }
        w
    }

    fn settle(body: &mut PlayerBody, w: &ClientWorld, ticks: usize) {
        for _ in 0..ticks {
            body.tick(w, Input::default());
        }
    }

    #[test]
    fn falls_and_lands_on_the_floor() {
        let w = flat_world();
        let mut b = PlayerBody::new(Vec3::new(8.5, 80.0, 8.5), 0.0, 0.0);
        settle(&mut b, &w, 80);
        assert_eq!(b.pos.y, 64.0);
        assert!(b.on_ground);
        assert_eq!(b.fall_distance, 0.0);
    }

    #[test]
    fn jump_peak_is_vanilla_1_2522_blocks() {
        let w = flat_world();
        let mut b = PlayerBody::new(Vec3::new(8.5, 64.0, 8.5), 0.0, 0.0);
        settle(&mut b, &w, 5);
        let mut peak = 0.0f64;
        for i in 0..40 {
            b.tick(&w, Input { jump: i == 0, ..Default::default() });
            peak = peak.max(b.pos.y - 64.0);
        }
        assert!((peak - 1.2522).abs() < 1.0E-3, "peak {peak}");
        assert!(b.on_ground);
    }

    #[test]
    fn walking_speeds_match_vanilla_blocks_per_second() {
        let w = flat_world();
        for (sprint, expected) in [(false, 4.317), (true, 5.612)] {
            let mut b = PlayerBody::new(Vec3::new(-10.0, 64.0, 8.5), -90.0, 0.0); // yaw -90 faces +x
            settle(&mut b, &w, 3);
            let start = b.pos.x;
            for _ in 0..40 {
                b.tick(&w, Input { forward: true, sprint, ..Default::default() });
            }
            let speed = (b.pos.x - start) / 40.0 * 20.0;
            // acceleration phase lowers the average slightly; terminal speed is within 6%
            assert!(speed > expected * 0.9 && speed < expected * 1.01, "sprint={sprint} speed {speed}");
        }
    }

    #[test]
    fn walls_stop_movement_and_steps_are_climbed() {
        let mut w = flat_world();
        for y in 64..67 {
            for z in 0..16 {
                w.set_block_state_id(12, y, z, stone());
            }
        }
        let mut b = PlayerBody::new(Vec3::new(8.5, 64.0, 8.5), -90.0, 0.0);
        settle(&mut b, &w, 3);
        for _ in 0..60 {
            b.tick(&w, Input { forward: true, ..Default::default() });
        }
        assert!((b.pos.x - (12.0 - 0.3)).abs() < 1.0E-6, "x {}", b.pos.x);
        assert!(b.horizontal_collision);
        // a one block step is not walkable without jumping; a slab (0.5) is
        let slab = Block::STONE_SLAB.default_state().with_property_str("type", "bottom").unwrap().id() as u16;
        let mut w2 = flat_world();
        for z in 0..16 {
            for x in 12..16 {
                w2.set_block_state_id(x, 64, z, slab);
            }
        }
        let mut b = PlayerBody::new(Vec3::new(8.5, 64.0, 8.5), -90.0, 0.0);
        settle(&mut b, &w2, 3);
        for _ in 0..24 {
            b.tick(&w2, Input { forward: true, ..Default::default() });
        }
        assert!(b.pos.x > 12.5 && b.pos.y == 64.5, "pos {:?}", b.pos);
    }

    #[test]
    fn sneaking_stops_at_ledges_and_creative_flight_has_no_gravity() {
        let mut w = flat_world();
        for x in 10..16 {
            for z in 0..16 {
                w.set_block_state_id(x, 63, z, Block::AIR.default_state().id() as u16);
            }
        }
        let mut b = PlayerBody::new(Vec3::new(8.5, 64.0, 8.5), -90.0, 0.0);
        settle(&mut b, &w, 3);
        for _ in 0..100 {
            b.tick(&w, Input { forward: true, shift: true, ..Default::default() });
        }
        assert!(b.pos.x < 10.4 + 1.0E-9 && b.pos.x > 10.0, "sneak x {}", b.pos.x);
        assert!(b.on_ground);

        let mut f = PlayerBody::new(Vec3::new(8.5, 80.0, 8.5), 0.0, 0.0);
        f.abilities.flying = true;
        settle(&mut f, &w, 40);
        assert!((f.pos.y - 80.0).abs() < 0.01, "flying y {}", f.pos.y);
        for _ in 0..10 {
            f.tick(&w, Input { jump: true, ..Default::default() });
        }
        assert!(f.pos.y > 81.0);
    }

    #[test]
    fn movement_packets_follow_local_player_rules() {
        let w = flat_world();
        let mut b = PlayerBody::new(Vec3::new(8.5, 64.0, 8.5), 0.0, 0.0);
        // the first ticks report on-ground transitions (velocity starts at zero); then nothing but the reminder
        for _ in 0..3 {
            b.tick(&w, Input::default());
            b.position_packet();
        }
        let mut sent = Vec::new();
        for _ in 0..25 {
            b.tick(&w, Input::default());
            sent.push(b.position_packet());
        }
        // standing still: only the 20-tick reminder is sent
        assert_eq!(sent.iter().filter(|p| p.is_some()).count(), 1, "{sent:?}");
        assert!(matches!(sent[16], Some(MoveOut::Pos { on_ground: true, .. })), "{sent:?}");
        b.yaw = 45.0;
        b.tick(&w, Input::default());
        assert!(matches!(b.position_packet(), Some(MoveOut::Rot { yaw, .. }) if yaw == 45.0));
    }
}
