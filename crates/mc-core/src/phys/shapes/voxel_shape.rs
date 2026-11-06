//! `VoxelShape` and its subclasses (`CubeVoxelShape`, `ArrayVoxelShape`, `SliceShape`).
//!
//! A `VoxelShape` is a cheap-to-clone handle. Vanilla compares shapes by reference in several places
//! (`first == second`, `shape == Shapes.block()`); that is [`VoxelShape::ptr_eq`]. `PartialEq` is instead
//! a *structural* comparison of the internal representation (kind + discrete bits/bounds + coords).

use std::sync::{Arc, OnceLock};

use super::coords::Coords;
use super::discrete::{DiscreteVoxelShape, SubShape};
use super::{BooleanOp, Shapes, fuzzy_equals};
use crate::direction::{Axis, AxisCycle, AxisDirection, Direction};
use crate::math::java::{max_f64, min_f64};
use crate::math::mth;
use crate::phys::{AABB, BlockHitResult, Vec3};
use crate::pos::{BlockPos, Vec3i};

struct Inner {
    shape: Arc<DiscreteVoxelShape>,
    coords: [Coords; 3],
    /// `CubeVoxelShape`: all coords are `CubePointRange`s and `findIndex` is a direct computation.
    cube: bool,
    /// Lazily computed `getFaceShape` results; `None` means "this shape itself".
    faces: [OnceLock<Option<VoxelShape>>; 6],
}

#[derive(Clone)]
pub struct VoxelShape(Arc<Inner>);

impl VoxelShape {
    fn make(shape: Arc<DiscreteVoxelShape>, coords: [Coords; 3], cube: bool) -> VoxelShape {
        VoxelShape(Arc::new(Inner {
            shape,
            coords,
            cube,
            faces: Default::default(),
        }))
    }

    /// `new CubeVoxelShape(shape)`
    pub fn cube(shape: impl Into<Arc<DiscreteVoxelShape>>) -> VoxelShape {
        let shape = shape.into();
        let coords = [Axis::X, Axis::Y, Axis::Z].map(|a| Coords::cube(shape.get_size(a)));
        Self::make(shape, coords, true)
    }

    /// `new ArrayVoxelShape(shape, xs, ys, zs)`; panics if the lengths are not `size + 1`.
    pub fn array(
        shape: impl Into<Arc<DiscreteVoxelShape>>,
        xs: impl Into<Coords>,
        ys: impl Into<Coords>,
        zs: impl Into<Coords>,
    ) -> VoxelShape {
        let shape = shape.into();
        let (xs, ys, zs) = (xs.into(), ys.into(), zs.into());
        assert!(
            shape.get_x_size() as usize + 1 == xs.size()
                && shape.get_y_size() as usize + 1 == ys.size()
                && shape.get_z_size() as usize + 1 == zs.size(),
            "Lengths of point arrays must be consistent with the size of the VoxelShape."
        );
        Self::make(shape, [xs, ys, zs], false)
    }

    /// `new SliceShape(self, axis, point)`
    pub fn slice(&self, axis: Axis, point: i32) -> VoxelShape {
        let d = &self.0.shape;
        let (xs, ys, zs) = (d.get_x_size(), d.get_y_size(), d.get_z_size());
        let sub = SubShape::new(
            self.0.shape.clone(),
            axis.choose(point, 0, 0),
            axis.choose(0, point, 0),
            axis.choose(0, 0, point),
            axis.choose(point + 1, xs, xs),
            axis.choose(ys, point + 1, ys),
            axis.choose(zs, zs, point + 1),
        );
        let coords = [Axis::X, Axis::Y, Axis::Z].map(|a| {
            if a == axis {
                Coords::Cube(1)
            } else {
                self.0.coords[a as usize].clone()
            }
        });
        Self::make(Arc::new(DiscreteVoxelShape::Sub(sub)), coords, false)
    }

    /// Reference identity (vanilla `==` / `Object.equals`).
    #[inline]
    pub fn ptr_eq(&self, other: &VoxelShape) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// The underlying `DiscreteVoxelShape` (`protected final shape`).
    pub fn shape(&self) -> &Arc<DiscreteVoxelShape> {
        &self.0.shape
    }

    /// Whether this is a `CubeVoxelShape`.
    pub fn is_cube_voxel_shape(&self) -> bool {
        self.0.cube
    }

    pub fn get_coords(&self, axis: Axis) -> &Coords {
        &self.0.coords[axis as usize]
    }

    fn get(&self, axis: Axis, i: i32) -> f64 {
        self.0.coords[axis as usize].get_double(i as usize)
    }

    pub fn min(&self, axis: Axis) -> f64 {
        let i = self.0.shape.first_full(axis);
        if i >= self.0.shape.get_size(axis) {
            f64::INFINITY
        } else {
            self.get(axis, i)
        }
    }

    pub fn max(&self, axis: Axis) -> f64 {
        let i = self.0.shape.last_full(axis);
        if i <= 0 {
            f64::NEG_INFINITY
        } else {
            self.get(axis, i)
        }
    }

    /// Panics for an empty shape (vanilla throws `UnsupportedOperationException`).
    pub fn bounds(&self) -> AABB {
        assert!(!self.is_empty(), "No bounds for empty shape.");
        AABB::new(
            self.min(Axis::X),
            self.min(Axis::Y),
            self.min(Axis::Z),
            self.max(Axis::X),
            self.max(Axis::Y),
            self.max(Axis::Z),
        )
    }

    pub fn single_encompassing(&self) -> VoxelShape {
        if self.is_empty() {
            Shapes::empty()
        } else {
            Shapes::box_(
                self.min(Axis::X),
                self.min(Axis::Y),
                self.min(Axis::Z),
                self.max(Axis::X),
                self.max(Axis::Y),
                self.max(Axis::Z),
            )
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.shape.is_empty()
    }

    pub fn move_vec(&self, delta: Vec3) -> VoxelShape {
        self.move_(delta.x, delta.y, delta.z)
    }

    pub fn move_vec3i(&self, delta: Vec3i) -> VoxelShape {
        self.move_(delta.x as f64, delta.y as f64, delta.z as f64)
    }

    pub fn move_(&self, dx: f64, dy: f64, dz: f64) -> VoxelShape {
        if self.is_empty() {
            Shapes::empty()
        } else {
            Self::make(
                self.0.shape.clone(),
                [
                    self.0.coords[0].offset(dx),
                    self.0.coords[1].offset(dy),
                    self.0.coords[2].offset(dz),
                ],
                false,
            )
        }
    }

    pub fn optimize(&self) -> VoxelShape {
        let mut result = Shapes::empty();
        self.for_all_boxes(|x1, y1, z1, x2, y2, z2| {
            result = Shapes::join_unoptimized(
                &result,
                &Shapes::box_(x1, y1, z1, x2, y2, z2),
                BooleanOp::OR,
            );
        });
        result
    }

    pub fn for_all_edges(&self, mut consumer: impl FnMut(f64, f64, f64, f64, f64, f64)) {
        self.0.shape.for_all_edges(
            &mut |xi1, yi1, zi1, xi2, yi2, zi2| {
                consumer(
                    self.get(Axis::X, xi1),
                    self.get(Axis::Y, yi1),
                    self.get(Axis::Z, zi1),
                    self.get(Axis::X, xi2),
                    self.get(Axis::Y, yi2),
                    self.get(Axis::Z, zi2),
                )
            },
            true,
        );
    }

    pub fn for_all_boxes(&self, mut consumer: impl FnMut(f64, f64, f64, f64, f64, f64)) {
        let [x, y, z] = &self.0.coords;
        self.0.shape.for_all_boxes(
            &mut |xi1, yi1, zi1, xi2, yi2, zi2| {
                consumer(
                    x.get_double(xi1 as usize),
                    y.get_double(yi1 as usize),
                    z.get_double(zi1 as usize),
                    x.get_double(xi2 as usize),
                    y.get_double(yi2 as usize),
                    z.get_double(zi2 as usize),
                )
            },
            true,
        );
    }

    pub fn to_aabbs(&self) -> Vec<AABB> {
        let mut list = Vec::new();
        self.for_all_boxes(|x1, y1, z1, x2, y2, z2| list.push(AABB::new(x1, y1, z1, x2, y2, z2)));
        list
    }

    /// `min(aAxis, b, c)`
    pub fn min_bc(&self, a_axis: Axis, b: f64, c: f64) -> f64 {
        let b_axis = AxisCycle::Forward.cycle_axis(a_axis);
        let c_axis = AxisCycle::Backward.cycle_axis(a_axis);
        let bi = self.find_index(b_axis, b);
        let ci = self.find_index(c_axis, c);
        let i = self.0.shape.first_full_bc(a_axis, bi, ci);
        if i >= self.0.shape.get_size(a_axis) {
            f64::INFINITY
        } else {
            self.get(a_axis, i)
        }
    }

    /// `max(aAxis, b, c)`
    pub fn max_bc(&self, a_axis: Axis, b: f64, c: f64) -> f64 {
        let b_axis = AxisCycle::Forward.cycle_axis(a_axis);
        let c_axis = AxisCycle::Backward.cycle_axis(a_axis);
        let bi = self.find_index(b_axis, b);
        let ci = self.find_index(c_axis, c);
        let i = self.0.shape.last_full_bc(a_axis, bi, ci);
        if i <= 0 {
            f64::NEG_INFINITY
        } else {
            self.get(a_axis, i)
        }
    }

    pub fn find_index(&self, axis: Axis, coord: f64) -> i32 {
        if self.0.cube {
            let size = self.0.shape.get_size(axis);
            mth::floor_f64(mth::clamp_f64(coord * size as f64, -1.0, size as f64))
        } else {
            mth::binary_search(0, self.0.shape.get_size(axis) + 1, |index| {
                coord < self.get(axis, index)
            }) - 1
        }
    }

    pub fn clip(&self, from: Vec3, to: Vec3, pos: BlockPos) -> Option<BlockHitResult> {
        if self.is_empty() {
            return None;
        }
        let diff = Vec3::new(to.x - from.x, to.y - from.y, to.z - from.z);
        if diff.x * diff.x + diff.y * diff.y + diff.z * diff.z < 1.0E-7 {
            return None;
        }
        let test_point = Vec3::new(
            from.x + diff.x * 0.001,
            from.y + diff.y * 0.001,
            from.z + diff.z * 0.001,
        );
        if self.0.shape.is_full_wide(
            self.find_index(Axis::X, test_point.x - pos.x() as f64),
            self.find_index(Axis::Y, test_point.y - pos.y() as f64),
            self.find_index(Axis::Z, test_point.z - pos.z() as f64),
        ) {
            Some(BlockHitResult {
                location: test_point,
                direction: Direction::get_approximate_nearest(diff.x, diff.y, diff.z)
                    .get_opposite(),
                block_pos: pos,
                miss: false,
                inside: true,
                world_border_hit: false,
            })
        } else {
            AABB::clip_boxes(&self.to_aabbs(), from, to, pos)
        }
    }

    pub fn closest_point_to(&self, point: Vec3) -> Option<Vec3> {
        if self.is_empty() {
            return None;
        }
        let dist_sqr = |p: Vec3, x: f64, y: f64, z: f64| {
            let (dx, dy, dz) = (p.x - x, p.y - y, p.z - z);
            dx * dx + dy * dy + dz * dz
        };
        let mut closest: Option<Vec3> = None;
        self.for_all_boxes(|x1, y1, z1, x2, y2, z2| {
            let x = mth::clamp_f64(point.x, x1, x2);
            let y = mth::clamp_f64(point.y, y1, y2);
            let z = mth::clamp_f64(point.z, z1, z2);
            if closest.is_none_or(|c| dist_sqr(point, x, y, z) < dist_sqr(point, c.x, c.y, c.z)) {
                closest = Some(Vec3::new(x, y, z));
            }
        });
        closest
    }

    pub fn get_face_shape(&self, direction: Direction) -> VoxelShape {
        if self.is_empty() || self.ptr_eq(Shapes::block_ref()) {
            return self.clone();
        }
        match self.0.faces[direction.ordinal()].get_or_init(|| self.calculate_face(direction)) {
            Some(face) => face.clone(),
            None => self.clone(),
        }
    }

    /// `None` stands for "this shape" (storing a self-handle would leak a reference cycle).
    fn calculate_face(&self, direction: Direction) -> Option<VoxelShape> {
        let axis = direction.get_axis();
        if self.is_cube_like_along(axis) {
            return None;
        }
        let sign = direction.get_axis_direction();
        let index = self.find_index(
            axis,
            if sign == AxisDirection::Positive {
                0.9999999
            } else {
                1.0E-7
            },
        );
        let slice = self.slice(axis, index);
        Some(if slice.is_empty() {
            Shapes::empty()
        } else if slice.is_cube_like() {
            Shapes::block()
        } else {
            slice
        })
    }

    pub fn is_cube_like(&self) -> bool {
        Axis::VALUES
            .iter()
            .all(|&axis| self.is_cube_like_along(axis))
    }

    fn is_cube_like_along(&self, axis: Axis) -> bool {
        let coords = self.get_coords(axis);
        coords.size() == 2
            && fuzzy_equals(coords.get_double(0), 0.0, 1.0E-7)
            && fuzzy_equals(coords.get_double(1), 1.0, 1.0E-7)
    }

    pub fn collide(&self, axis: Axis, moving: &AABB, distance: f64) -> f64 {
        self.collide_x(AxisCycle::between(axis, Axis::X), moving, distance)
    }

    pub fn collide_x(&self, transform: AxisCycle, moving: &AABB, mut distance: f64) -> f64 {
        if self.is_empty() {
            return distance;
        }
        if distance.abs() < 1.0E-7 {
            return 0.0;
        }
        let shape = &self.0.shape;
        let inverse = transform.inverse();
        let a_axis = inverse.cycle_axis(Axis::X);
        let b_axis = inverse.cycle_axis(Axis::Y);
        let c_axis = inverse.cycle_axis(Axis::Z);
        let max_a = moving.max(a_axis);
        let min_a = moving.min(a_axis);
        let a_min = self.find_index(a_axis, min_a + 1.0E-7);
        let a_max = self.find_index(a_axis, max_a - 1.0E-7);
        let b_min = 0.max(self.find_index(b_axis, moving.min(b_axis) + 1.0E-7));
        let b_max = shape
            .get_size(b_axis)
            .min(self.find_index(b_axis, moving.max(b_axis) - 1.0E-7) + 1);
        let c_min = 0.max(self.find_index(c_axis, moving.min(c_axis) + 1.0E-7));
        let c_max = shape
            .get_size(c_axis)
            .min(self.find_index(c_axis, moving.max(c_axis) - 1.0E-7) + 1);
        let a_size = shape.get_size(a_axis);
        if distance > 0.0 {
            for a in (a_max + 1)..a_size {
                for b in b_min..b_max {
                    for c in c_min..c_max {
                        if shape.is_full_wide_cycled(inverse, a, b, c) {
                            let new_distance = self.get(a_axis, a) - max_a;
                            if new_distance >= -1.0E-7 {
                                distance = min_f64(distance, new_distance);
                            }
                            return distance;
                        }
                    }
                }
            }
        } else if distance < 0.0 {
            for a in (0..a_min).rev() {
                for b in b_min..b_max {
                    for c in c_min..c_max {
                        if shape.is_full_wide_cycled(inverse, a, b, c) {
                            let new_distance = self.get(a_axis, a + 1) - min_a;
                            if new_distance <= 1.0E-7 {
                                distance = max_f64(distance, new_distance);
                            }
                            return distance;
                        }
                    }
                }
            }
        }
        distance
    }
}

/// Structural equality (kind, discrete shape, coordinates). NOT vanilla's identity `equals`; see [`VoxelShape::ptr_eq`].
impl PartialEq for VoxelShape {
    fn eq(&self, other: &VoxelShape) -> bool {
        self.ptr_eq(other)
            || (self.0.cube == other.0.cube
                && self.0.coords == other.0.coords
                && self.0.shape == other.0.shape)
    }
}
impl Eq for VoxelShape {}

impl std::fmt::Debug for VoxelShape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoxelShape")
            .field("cube", &self.0.cube)
            .field("shape", &self.0.shape)
            .field("coords", &self.0.coords)
            .finish()
    }
}
