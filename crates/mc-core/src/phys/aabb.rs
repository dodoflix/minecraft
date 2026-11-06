//! `world.phys.AABB` (+ the `BlockHitResult` produced by `AABB.clip(Iterable<AABB>, ..)`).
//! Not ported: `move(Vector3f)`; `BoundingBox` conversion takes the six integer bounds instead.

use crate::direction::{Axis, Direction};
use crate::math::java::java_double_to_string;
use crate::math::mth;
use crate::phys::Vec3;
use crate::phys::vec3::{double_hash, double_to_long_bits};
use crate::pos::{BlockPos, Vec3i};
use std::fmt;
use std::hash::{Hash, Hasher};

const EPSILON: f64 = 1.0E-7;

/// `Math.min(double, double)`: NaN-propagating, `-0.0 < 0.0`.
fn java_min(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        a
    } else if a == 0.0 && b == 0.0 && b.is_sign_negative() {
        b
    } else if a <= b {
        a
    } else {
        b
    }
}

/// `Math.max(double, double)`: NaN-propagating, `-0.0 < 0.0`.
fn java_max(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        a
    } else if a == 0.0 && b == 0.0 && a.is_sign_negative() {
        b
    } else if a >= b {
        a
    } else {
        b
    }
}

/// The subset of `BlockHitResult` produced by [`AABB::clip_boxes`] (and consumed by `VoxelShape.clip`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockHitResult {
    pub location: Vec3,
    pub direction: Direction,
    pub block_pos: BlockPos,
    pub miss: bool,
    pub inside: bool,
    pub world_border_hit: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct AABB {
    pub min_x: f64,
    pub min_y: f64,
    pub min_z: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub max_z: f64,
}

/// `Double.compare(..) == 0` per component: `-0.0 != 0.0`, `NaN == NaN`.
impl PartialEq for AABB {
    fn eq(&self, other: &Self) -> bool {
        [
            self.min_x, self.min_y, self.min_z, self.max_x, self.max_y, self.max_z,
        ]
        .into_iter()
        .zip([
            other.min_x,
            other.min_y,
            other.min_z,
            other.max_x,
            other.max_y,
            other.max_z,
        ])
        .all(|(a, b)| double_to_long_bits(a) == double_to_long_bits(b))
    }
}
impl Eq for AABB {}

impl Hash for AABB {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for v in [
            self.min_x, self.min_y, self.min_z, self.max_x, self.max_y, self.max_z,
        ] {
            double_to_long_bits(v).hash(state);
        }
    }
}

impl AABB {
    /// Sorts each axis pair into min/max exactly as vanilla (`Math.min`/`Math.max`).
    pub fn new(min_x: f64, min_y: f64, min_z: f64, max_x: f64, max_y: f64, max_z: f64) -> AABB {
        AABB {
            min_x: java_min(min_x, max_x),
            min_y: java_min(min_y, max_y),
            min_z: java_min(min_z, max_z),
            max_x: java_max(min_x, max_x),
            max_y: java_max(min_y, max_y),
            max_z: java_max(min_z, max_z),
        }
    }

    /// `new AABB(BlockPos)`
    pub fn from_block_pos(pos: &Vec3i) -> AABB {
        AABB::new(
            pos.x() as f64,
            pos.y() as f64,
            pos.z() as f64,
            pos.x().wrapping_add(1) as f64,
            pos.y().wrapping_add(1) as f64,
            pos.z().wrapping_add(1) as f64,
        )
    }

    /// `new AABB(Vec3, Vec3)`
    pub fn from_corners(begin: Vec3, end: Vec3) -> AABB {
        AABB::new(begin.x, begin.y, begin.z, end.x, end.y, end.z)
    }

    /// `AABB.of(BoundingBox)` with the box's inclusive integer bounds.
    pub fn of_box(min_x: i32, min_y: i32, min_z: i32, max_x: i32, max_y: i32, max_z: i32) -> AABB {
        AABB::new(
            min_x as f64,
            min_y as f64,
            min_z as f64,
            max_x.wrapping_add(1) as f64,
            max_y.wrapping_add(1) as f64,
            max_z.wrapping_add(1) as f64,
        )
    }

    pub fn unit_cube_from_lower_corner(pos: Vec3) -> AABB {
        AABB::new(pos.x, pos.y, pos.z, pos.x + 1.0, pos.y + 1.0, pos.z + 1.0)
    }

    pub fn encapsulating_full_blocks(pos0: &Vec3i, pos1: &Vec3i) -> AABB {
        AABB::new(
            pos0.x().min(pos1.x()) as f64,
            pos0.y().min(pos1.y()) as f64,
            pos0.z().min(pos1.z()) as f64,
            pos0.x().max(pos1.x()).wrapping_add(1) as f64,
            pos0.y().max(pos1.y()).wrapping_add(1) as f64,
            pos0.z().max(pos1.z()).wrapping_add(1) as f64,
        )
    }

    pub fn set_min_x(&self, min_x: f64) -> AABB {
        AABB::new(
            min_x, self.min_y, self.min_z, self.max_x, self.max_y, self.max_z,
        )
    }
    pub fn set_min_y(&self, min_y: f64) -> AABB {
        AABB::new(
            self.min_x, min_y, self.min_z, self.max_x, self.max_y, self.max_z,
        )
    }
    pub fn set_min_z(&self, min_z: f64) -> AABB {
        AABB::new(
            self.min_x, self.min_y, min_z, self.max_x, self.max_y, self.max_z,
        )
    }
    pub fn set_max_x(&self, max_x: f64) -> AABB {
        AABB::new(
            self.min_x, self.min_y, self.min_z, max_x, self.max_y, self.max_z,
        )
    }
    pub fn set_max_y(&self, max_y: f64) -> AABB {
        AABB::new(
            self.min_x, self.min_y, self.min_z, self.max_x, max_y, self.max_z,
        )
    }
    pub fn set_max_z(&self, max_z: f64) -> AABB {
        AABB::new(
            self.min_x, self.min_y, self.min_z, self.max_x, self.max_y, max_z,
        )
    }

    pub fn min(&self, axis: Axis) -> f64 {
        axis.choose(self.min_x, self.min_y, self.min_z)
    }

    pub fn max(&self, axis: Axis) -> f64 {
        axis.choose(self.max_x, self.max_y, self.max_z)
    }

    /// Java `hashCode()`
    pub fn hash_code(&self) -> i32 {
        let mut result = double_hash(self.min_x);
        for v in [self.min_y, self.min_z, self.max_x, self.max_y, self.max_z] {
            result = result.wrapping_mul(31).wrapping_add(double_hash(v));
        }
        result
    }

    pub fn contract(&self, xa: f64, ya: f64, za: f64) -> AABB {
        let (mut min_x, mut min_y, mut min_z) = (self.min_x, self.min_y, self.min_z);
        let (mut max_x, mut max_y, mut max_z) = (self.max_x, self.max_y, self.max_z);
        if xa < 0.0 {
            min_x -= xa;
        } else if xa > 0.0 {
            max_x -= xa;
        }
        if ya < 0.0 {
            min_y -= ya;
        } else if ya > 0.0 {
            max_y -= ya;
        }
        if za < 0.0 {
            min_z -= za;
        } else if za > 0.0 {
            max_z -= za;
        }
        AABB::new(min_x, min_y, min_z, max_x, max_y, max_z)
    }

    /// `expandTowards(Vec3)`
    pub fn expand_towards_vec3(&self, delta: Vec3) -> AABB {
        self.expand_towards(delta.x, delta.y, delta.z)
    }

    pub fn expand_towards(&self, xa: f64, ya: f64, za: f64) -> AABB {
        let (mut min_x, mut min_y, mut min_z) = (self.min_x, self.min_y, self.min_z);
        let (mut max_x, mut max_y, mut max_z) = (self.max_x, self.max_y, self.max_z);
        if xa < 0.0 {
            min_x += xa;
        } else if xa > 0.0 {
            max_x += xa;
        }
        if ya < 0.0 {
            min_y += ya;
        } else if ya > 0.0 {
            max_y += ya;
        }
        if za < 0.0 {
            min_z += za;
        } else if za > 0.0 {
            max_z += za;
        }
        AABB::new(min_x, min_y, min_z, max_x, max_y, max_z)
    }

    pub fn inflate(&self, x_add: f64, y_add: f64, z_add: f64) -> AABB {
        AABB::new(
            self.min_x - x_add,
            self.min_y - y_add,
            self.min_z - z_add,
            self.max_x + x_add,
            self.max_y + y_add,
            self.max_z + z_add,
        )
    }

    /// `inflate(double)`
    pub fn inflate_scalar(&self, amount_to_add_in_all_directions: f64) -> AABB {
        self.inflate(
            amount_to_add_in_all_directions,
            amount_to_add_in_all_directions,
            amount_to_add_in_all_directions,
        )
    }

    pub fn intersect(&self, other: &AABB) -> AABB {
        AABB::new(
            java_max(self.min_x, other.min_x),
            java_max(self.min_y, other.min_y),
            java_max(self.min_z, other.min_z),
            java_min(self.max_x, other.max_x),
            java_min(self.max_y, other.max_y),
            java_min(self.max_z, other.max_z),
        )
    }

    pub fn minmax(&self, other: &AABB) -> AABB {
        AABB::new(
            java_min(self.min_x, other.min_x),
            java_min(self.min_y, other.min_y),
            java_min(self.min_z, other.min_z),
            java_max(self.max_x, other.max_x),
            java_max(self.max_y, other.max_y),
            java_max(self.max_z, other.max_z),
        )
    }

    /// `move(double, double, double)`
    pub fn move_(&self, xa: f64, ya: f64, za: f64) -> AABB {
        AABB::new(
            self.min_x + xa,
            self.min_y + ya,
            self.min_z + za,
            self.max_x + xa,
            self.max_y + ya,
            self.max_z + za,
        )
    }

    /// `move(BlockPos)`
    pub fn move_block_pos(&self, pos: &Vec3i) -> AABB {
        let (x, y, z) = (pos.x() as f64, pos.y() as f64, pos.z() as f64);
        AABB::new(
            self.min_x + x,
            self.min_y + y,
            self.min_z + z,
            self.max_x + x,
            self.max_y + y,
            self.max_z + z,
        )
    }

    /// `move(Vec3)`
    pub fn move_vec(&self, pos: Vec3) -> AABB {
        self.move_(pos.x, pos.y, pos.z)
    }

    /// `intersects(AABB)`
    pub fn intersects_aabb(&self, aabb: &AABB) -> bool {
        self.intersects_coords(
            aabb.min_x, aabb.min_y, aabb.min_z, aabb.max_x, aabb.max_y, aabb.max_z,
        )
    }

    /// `intersects(double x6)`
    pub fn intersects_coords(
        &self,
        min_x: f64,
        min_y: f64,
        min_z: f64,
        max_x: f64,
        max_y: f64,
        max_z: f64,
    ) -> bool {
        self.min_x < max_x
            && self.max_x > min_x
            && self.min_y < max_y
            && self.max_y > min_y
            && self.min_z < max_z
            && self.max_z > min_z
    }

    /// `intersects(Vec3, Vec3)`
    pub fn intersects_vec3(&self, min: Vec3, max: Vec3) -> bool {
        self.intersects_coords(
            java_min(min.x, max.x),
            java_min(min.y, max.y),
            java_min(min.z, max.z),
            java_max(min.x, max.x),
            java_max(min.y, max.y),
            java_max(min.z, max.z),
        )
    }

    /// `intersects(BlockPos)`
    pub fn intersects_block_pos(&self, pos: &Vec3i) -> bool {
        self.intersects_coords(
            pos.x() as f64,
            pos.y() as f64,
            pos.z() as f64,
            pos.x().wrapping_add(1) as f64,
            pos.y().wrapping_add(1) as f64,
            pos.z().wrapping_add(1) as f64,
        )
    }

    /// `contains(Vec3)`
    pub fn contains_vec3(&self, vec: Vec3) -> bool {
        self.contains(vec.x, vec.y, vec.z)
    }

    pub fn contains(&self, x: f64, y: f64, z: f64) -> bool {
        x >= self.min_x
            && x < self.max_x
            && y >= self.min_y
            && y < self.max_y
            && z >= self.min_z
            && z < self.max_z
    }

    pub fn get_size(&self) -> f64 {
        let xs = self.get_xsize();
        let ys = self.get_ysize();
        let zs = self.get_zsize();
        (xs + ys + zs) / 3.0
    }

    pub fn get_xsize(&self) -> f64 {
        self.max_x - self.min_x
    }
    pub fn get_ysize(&self) -> f64 {
        self.max_y - self.min_y
    }
    pub fn get_zsize(&self) -> f64 {
        self.max_z - self.min_z
    }

    pub fn deflate(&self, x_subtract: f64, y_subtract: f64, z_subtract: f64) -> AABB {
        self.inflate(-x_subtract, -y_subtract, -z_subtract)
    }

    /// `deflate(double)`
    pub fn deflate_scalar(&self, amount: f64) -> AABB {
        self.inflate_scalar(-amount)
    }

    /// Instance `clip(Vec3 from, Vec3 to)`.
    pub fn clip(&self, from: Vec3, to: Vec3) -> Option<Vec3> {
        Self::clip_box_coords(
            self.min_x, self.min_y, self.min_z, self.max_x, self.max_y, self.max_z, from, to,
        )
    }

    /// Static `clip(double x6, Vec3 from, Vec3 to)`.
    #[allow(clippy::too_many_arguments)]
    pub fn clip_box_coords(
        min_x: f64,
        min_y: f64,
        min_z: f64,
        max_x: f64,
        max_y: f64,
        max_z: f64,
        from: Vec3,
        to: Vec3,
    ) -> Option<Vec3> {
        let mut scale_reference = 1.0;
        let dx = to.x - from.x;
        let dy = to.y - from.y;
        let dz = to.z - from.z;
        let direction = Self::get_direction_coords(
            min_x,
            min_y,
            min_z,
            max_x,
            max_y,
            max_z,
            from,
            &mut scale_reference,
            None,
            dx,
            dy,
            dz,
        );
        direction?;
        let scale = scale_reference;
        Some(from.add_xyz(scale * dx, scale * dy, scale * dz))
    }

    /// Static `clip(Iterable<AABB>, from, to, pos)`; `None` is vanilla's `null`.
    pub fn clip_boxes(
        boxes: &[AABB],
        from: Vec3,
        to: Vec3,
        pos: BlockPos,
    ) -> Option<BlockHitResult> {
        let mut scale_reference = 1.0;
        let mut direction = None;
        let dx = to.x - from.x;
        let dy = to.y - from.y;
        let dz = to.z - from.z;
        for aabb in boxes {
            direction = Self::get_direction(
                &aabb.move_block_pos(&pos),
                from,
                &mut scale_reference,
                direction,
                dx,
                dy,
                dz,
            );
        }
        let direction = direction?;
        let scale = scale_reference;
        Some(BlockHitResult {
            location: from.add_xyz(scale * dx, scale * dy, scale * dz),
            direction,
            block_pos: pos,
            miss: false,
            inside: false,
            world_border_hit: false,
        })
    }

    /// Vanilla-private `getDirection(AABB, ..)`.
    pub fn get_direction(
        aabb: &AABB,
        from: Vec3,
        scale_reference: &mut f64,
        direction: Option<Direction>,
        dx: f64,
        dy: f64,
        dz: f64,
    ) -> Option<Direction> {
        Self::get_direction_coords(
            aabb.min_x,
            aabb.min_y,
            aabb.min_z,
            aabb.max_x,
            aabb.max_y,
            aabb.max_z,
            from,
            scale_reference,
            direction,
            dx,
            dy,
            dz,
        )
    }

    /// Vanilla-private `getDirection(double x6, ..)`.
    #[allow(clippy::too_many_arguments)]
    pub fn get_direction_coords(
        min_x: f64,
        min_y: f64,
        min_z: f64,
        max_x: f64,
        max_y: f64,
        max_z: f64,
        from: Vec3,
        scale_reference: &mut f64,
        mut direction: Option<Direction>,
        dx: f64,
        dy: f64,
        dz: f64,
    ) -> Option<Direction> {
        if dx > EPSILON {
            direction = Self::clip_point(
                scale_reference,
                direction,
                dx,
                dy,
                dz,
                min_x,
                min_y,
                max_y,
                min_z,
                max_z,
                Direction::West,
                from.x,
                from.y,
                from.z,
            );
        } else if dx < -EPSILON {
            direction = Self::clip_point(
                scale_reference,
                direction,
                dx,
                dy,
                dz,
                max_x,
                min_y,
                max_y,
                min_z,
                max_z,
                Direction::East,
                from.x,
                from.y,
                from.z,
            );
        }

        if dy > EPSILON {
            direction = Self::clip_point(
                scale_reference,
                direction,
                dy,
                dz,
                dx,
                min_y,
                min_z,
                max_z,
                min_x,
                max_x,
                Direction::Down,
                from.y,
                from.z,
                from.x,
            );
        } else if dy < -EPSILON {
            direction = Self::clip_point(
                scale_reference,
                direction,
                dy,
                dz,
                dx,
                max_y,
                min_z,
                max_z,
                min_x,
                max_x,
                Direction::Up,
                from.y,
                from.z,
                from.x,
            );
        }

        if dz > EPSILON {
            direction = Self::clip_point(
                scale_reference,
                direction,
                dz,
                dx,
                dy,
                min_z,
                min_x,
                max_x,
                min_y,
                max_y,
                Direction::North,
                from.z,
                from.x,
                from.y,
            );
        } else if dz < -EPSILON {
            direction = Self::clip_point(
                scale_reference,
                direction,
                dz,
                dx,
                dy,
                max_z,
                min_x,
                max_x,
                min_y,
                max_y,
                Direction::South,
                from.z,
                from.x,
                from.y,
            );
        }

        direction
    }

    /// Vanilla-private `clipPoint`.
    #[allow(clippy::too_many_arguments)]
    pub fn clip_point(
        scale_reference: &mut f64,
        direction: Option<Direction>,
        da: f64,
        db: f64,
        dc: f64,
        point: f64,
        min_b: f64,
        max_b: f64,
        min_c: f64,
        max_c: f64,
        new_direction: Direction,
        from_a: f64,
        from_b: f64,
        from_c: f64,
    ) -> Option<Direction> {
        let s = (point - from_a) / da;
        let pb = from_b + s * db;
        let pc = from_c + s * dc;
        if 0.0 < s
            && s < *scale_reference
            && min_b - EPSILON < pb
            && pb < max_b + EPSILON
            && min_c - EPSILON < pc
            && pc < max_c + EPSILON
        {
            *scale_reference = s;
            Some(new_direction)
        } else {
            direction
        }
    }

    pub fn collided_along_vector(&self, vector: Vec3, aabbs: &[AABB]) -> bool {
        let from = self.get_center();
        let to = from.add(vector);
        for shape_part in aabbs {
            let inflated = shape_part.inflate(
                self.get_xsize() * 0.5 - EPSILON,
                self.get_ysize() * 0.5 - EPSILON,
                self.get_zsize() * 0.5 - EPSILON,
            );
            if inflated.contains_vec3(to) || inflated.contains_vec3(from) {
                return true;
            }
            if inflated.clip(from, to).is_some() {
                return true;
            }
        }
        false
    }

    /// `distanceToSqr(Vec3)`
    pub fn distance_to_sqr(&self, point: Vec3) -> f64 {
        let dx = java_max(java_max(self.min_x - point.x, point.x - self.max_x), 0.0);
        let dy = java_max(java_max(self.min_y - point.y, point.y - self.max_y), 0.0);
        let dz = java_max(java_max(self.min_z - point.z, point.z - self.max_z), 0.0);
        mth::length_squared_xyz_f64(dx, dy, dz)
    }

    /// `distanceToSqr(AABB)`
    pub fn distance_to_sqr_aabb(&self, bb: &AABB) -> f64 {
        let dx = java_max(java_max(self.min_x - bb.max_x, bb.min_x - self.max_x), 0.0);
        let dy = java_max(java_max(self.min_y - bb.max_y, bb.min_y - self.max_y), 0.0);
        let dz = java_max(java_max(self.min_z - bb.max_z, bb.min_z - self.max_z), 0.0);
        mth::length_squared_xyz_f64(dx, dy, dz)
    }

    pub fn has_nan(&self) -> bool {
        self.min_x.is_nan()
            || self.min_y.is_nan()
            || self.min_z.is_nan()
            || self.max_x.is_nan()
            || self.max_y.is_nan()
            || self.max_z.is_nan()
    }

    pub fn get_center(&self) -> Vec3 {
        Vec3::new(
            mth::lerp_f64(0.5, self.min_x, self.max_x),
            mth::lerp_f64(0.5, self.min_y, self.max_y),
            mth::lerp_f64(0.5, self.min_z, self.max_z),
        )
    }

    pub fn get_bottom_center(&self) -> Vec3 {
        Vec3::new(
            mth::lerp_f64(0.5, self.min_x, self.max_x),
            self.min_y,
            mth::lerp_f64(0.5, self.min_z, self.max_z),
        )
    }

    pub fn get_min_position(&self) -> Vec3 {
        Vec3::new(self.min_x, self.min_y, self.min_z)
    }

    pub fn get_max_position(&self) -> Vec3 {
        Vec3::new(self.max_x, self.max_y, self.max_z)
    }

    pub fn of_size(center: Vec3, size_x: f64, size_y: f64, size_z: f64) -> AABB {
        AABB::new(
            center.x - size_x / 2.0,
            center.y - size_y / 2.0,
            center.z - size_z / 2.0,
            center.x + size_x / 2.0,
            center.y + size_y / 2.0,
            center.z + size_z / 2.0,
        )
    }

    pub fn next_deflated(&self) -> AABB {
        AABB::new(
            self.min_x.next_up(),
            self.min_y.next_up(),
            self.min_z.next_up(),
            self.max_x.next_down(),
            self.max_y.next_down(),
            self.max_z.next_down(),
        )
    }
}

/// Vanilla `AABB.toString` with Java `Double.toString` number formatting.
impl fmt::Display for AABB {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d, e, g] = [
            self.min_x, self.min_y, self.min_z, self.max_x, self.max_y, self.max_z,
        ]
        .map(java_double_to_string);
        write!(f, "AABB[{a}, {b}, {c}] -> [{d}, {e}, {g}]")
    }
}

/// `AABB.Builder` (float accumulation over points).
#[derive(Clone, Copy, Debug)]
pub struct Builder {
    min_x: f32,
    min_y: f32,
    min_z: f32,
    max_x: f32,
    max_y: f32,
    max_z: f32,
    defined: bool,
}

impl Default for Builder {
    fn default() -> Self {
        Builder {
            min_x: f32::INFINITY,
            min_y: f32::INFINITY,
            min_z: f32::INFINITY,
            max_x: f32::NEG_INFINITY,
            max_y: f32::NEG_INFINITY,
            max_z: f32::NEG_INFINITY,
            defined: false,
        }
    }
}

/// `Math.min(float, float)` / `Math.max(float, float)`: NaN-propagating, `-0.0 < 0.0`.
fn java_min_f32(a: f32, b: f32) -> f32 {
    if a.is_nan() {
        a
    } else if a == 0.0 && b == 0.0 && b.is_sign_negative() {
        b
    } else if a <= b {
        a
    } else {
        b
    }
}

fn java_max_f32(a: f32, b: f32) -> f32 {
    if a.is_nan() {
        a
    } else if a == 0.0 && b == 0.0 && a.is_sign_negative() {
        b
    } else if a >= b {
        a
    } else {
        b
    }
}

impl Builder {
    /// `include(Vector3fc)`
    pub fn include(&mut self, x: f32, y: f32, z: f32) {
        self.min_x = java_min_f32(self.min_x, x);
        self.min_y = java_min_f32(self.min_y, y);
        self.min_z = java_min_f32(self.min_z, z);
        self.max_x = java_max_f32(self.max_x, x);
        self.max_y = java_max_f32(self.max_y, y);
        self.max_z = java_max_f32(self.max_z, z);
        self.defined = true;
    }

    pub fn is_defined(&self) -> bool {
        self.defined
    }

    /// Panics if no point was included (vanilla: `IllegalStateException`).
    pub fn build(&self) -> AABB {
        assert!(
            self.defined,
            "Cannot build an undefined AABB. Include at least one point."
        );
        AABB::new(
            self.min_x as f64,
            self.min_y as f64,
            self.min_z as f64,
            self.max_x as f64,
            self.max_y as f64,
            self.max_z as f64,
        )
    }
}
