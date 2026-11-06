//! `Shapes`: shape construction, boolean joins and occlusion tests.
//!
//! The `rotate*` family returns `HashMap`s where vanilla returns `EnumMap`s.

use std::collections::HashMap;
use std::sync::LazyLock;

use super::coords::{Coords, IndexMerger, lcm};
use super::discrete::{BitSetDiscreteVoxelShape, DiscreteVoxelShape};
use super::{BooleanOp, VoxelShape, fuzzy_equals};
use crate::direction::{Axis, AxisDirection, Direction};
use crate::math::java::round_f64 as java_round;
use crate::math::octahedral_group::OctahedralGroup;
use crate::phys::{AABB, Vec3};

static BLOCK: LazyLock<VoxelShape> = LazyLock::new(|| {
    let mut shape = BitSetDiscreteVoxelShape::new(1, 1, 1);
    shape.fill(0, 0, 0);
    VoxelShape::cube(DiscreteVoxelShape::BitSet(shape))
});

static EMPTY: LazyLock<VoxelShape> = LazyLock::new(|| {
    VoxelShape::array(
        DiscreteVoxelShape::BitSet(BitSetDiscreteVoxelShape::new(0, 0, 0)),
        vec![0.0],
        vec![0.0],
        vec![0.0],
    )
});

static INFINITY: LazyLock<VoxelShape> = LazyLock::new(|| {
    Shapes::box_(
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::INFINITY,
        f64::INFINITY,
    )
});

/// `Shapes.BLOCK_CENTER`
const BLOCK_CENTER: Vec3 = Vec3 {
    x: 0.5,
    y: 0.5,
    z: 0.5,
};

/// `world.level.block.state.properties.AttachFace`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AttachFace {
    Floor,
    Wall,
    Ceiling,
}

impl AttachFace {
    pub const ALL: [AttachFace; 3] = [AttachFace::Floor, AttachFace::Wall, AttachFace::Ceiling];

    pub const fn get_serialized_name(self) -> &'static str {
        match self {
            AttachFace::Floor => "floor",
            AttachFace::Wall => "wall",
            AttachFace::Ceiling => "ceiling",
        }
    }

    pub fn by_name(name: &str) -> Option<AttachFace> {
        Self::ALL
            .into_iter()
            .find(|f| f.get_serialized_name() == name)
    }
}

pub struct Shapes;

impl Shapes {
    pub const EPSILON: f64 = 1.0E-7;
    pub const BIG_EPSILON: f64 = 1.0E-6;

    /// `Shapes.INFINITY`
    pub fn infinity() -> VoxelShape {
        INFINITY.clone()
    }

    pub fn empty() -> VoxelShape {
        EMPTY.clone()
    }

    pub fn block() -> VoxelShape {
        BLOCK.clone()
    }

    pub(crate) fn block_ref() -> &'static VoxelShape {
        &BLOCK
    }

    /// `Shapes.box`: panics if a min exceeds its max (vanilla throws `IllegalArgumentException`).
    pub fn box_(
        min_x: f64,
        min_y: f64,
        min_z: f64,
        max_x: f64,
        max_y: f64,
        max_z: f64,
    ) -> VoxelShape {
        if !(min_x > max_x) && !(min_y > max_y) && !(min_z > max_z) {
            Self::create_bounds(min_x, min_y, min_z, max_x, max_y, max_z)
        } else {
            panic!("The min values need to be smaller or equals to the max values");
        }
    }

    /// `Shapes.create(double x6)`
    pub fn create_bounds(
        min_x: f64,
        min_y: f64,
        min_z: f64,
        max_x: f64,
        max_y: f64,
        max_z: f64,
    ) -> VoxelShape {
        if !(max_x - min_x < 1.0E-7) && !(max_y - min_y < 1.0E-7) && !(max_z - min_z < 1.0E-7) {
            let x_bits = Self::find_bits(min_x, max_x);
            let y_bits = Self::find_bits(min_y, max_y);
            let z_bits = Self::find_bits(min_z, max_z);
            if x_bits < 0 || y_bits < 0 || z_bits < 0 {
                VoxelShape::array(
                    BLOCK.shape().clone(),
                    vec![min_x, max_x],
                    vec![min_y, max_y],
                    vec![min_z, max_z],
                )
            } else if x_bits == 0 && y_bits == 0 && z_bits == 0 {
                Self::block()
            } else {
                let x_size = 1 << x_bits;
                let y_size = 1 << y_bits;
                let z_size = 1 << z_bits;
                let voxel_shape = BitSetDiscreteVoxelShape::with_filled_bounds(
                    x_size,
                    y_size,
                    z_size,
                    java_round(min_x * x_size as f64) as i32,
                    java_round(min_y * y_size as f64) as i32,
                    java_round(min_z * z_size as f64) as i32,
                    java_round(max_x * x_size as f64) as i32,
                    java_round(max_y * y_size as f64) as i32,
                    java_round(max_z * z_size as f64) as i32,
                );
                VoxelShape::cube(DiscreteVoxelShape::BitSet(voxel_shape))
            }
        } else {
            Self::empty()
        }
    }

    /// `Shapes.create(AABB)`
    pub fn create(aabb: AABB) -> VoxelShape {
        Self::create_bounds(
            aabb.min_x, aabb.min_y, aabb.min_z, aabb.max_x, aabb.max_y, aabb.max_z,
        )
    }

    pub fn find_bits(min: f64, max: f64) -> i32 {
        if !(min < -1.0E-7) && !(max > 1.0000001) {
            for bits in 0..=3 {
                let intervals = 1 << bits;
                let sh_min = min * intervals as f64;
                let sh_max = max * intervals as f64;
                let found_min =
                    (sh_min - java_round(sh_min) as f64).abs() < 1.0E-7 * intervals as f64;
                let found_max =
                    (sh_max - java_round(sh_max) as f64).abs() < 1.0E-7 * intervals as f64;
                if found_min && found_max {
                    return bits;
                }
            }
        }
        -1
    }

    pub fn lcm(first: i32, second: i32) -> i64 {
        lcm(first, second)
    }

    pub fn or(first: &VoxelShape, second: &VoxelShape) -> VoxelShape {
        Self::join(first, second, BooleanOp::OR)
    }

    /// `Shapes.or(first, tail...)`
    pub fn or_many(first: &VoxelShape, tail: &[VoxelShape]) -> VoxelShape {
        tail.iter().fold(first.clone(), |acc, s| Self::or(&acc, s))
    }

    pub fn join(first: &VoxelShape, second: &VoxelShape, op: BooleanOp) -> VoxelShape {
        Self::join_unoptimized(first, second, op).optimize()
    }

    pub fn join_unoptimized(first: &VoxelShape, second: &VoxelShape, op: BooleanOp) -> VoxelShape {
        assert!(
            !op.apply(false, false),
            "op.apply(false, false) must be false"
        );
        if first.ptr_eq(second) {
            return if op.apply(true, true) {
                first.clone()
            } else {
                Self::empty()
            };
        }
        let first_only_matters = op.apply(true, false);
        let second_only_matters = op.apply(false, true);
        if first.is_empty() {
            return if second_only_matters {
                second.clone()
            } else {
                Self::empty()
            };
        }
        if second.is_empty() {
            return if first_only_matters {
                first.clone()
            } else {
                Self::empty()
            };
        }
        let (x_merger, y_merger, z_merger) =
            Self::merge_all(first, second, first_only_matters, second_only_matters);
        let voxel_shape = BitSetDiscreteVoxelShape::join(
            first.shape(),
            second.shape(),
            &x_merger,
            &y_merger,
            &z_merger,
            op,
        );
        if x_merger.is_discrete_cube() && y_merger.is_discrete_cube() && z_merger.is_discrete_cube()
        {
            VoxelShape::cube(DiscreteVoxelShape::BitSet(voxel_shape))
        } else {
            VoxelShape::array(
                DiscreteVoxelShape::BitSet(voxel_shape),
                x_merger.get_list(),
                y_merger.get_list(),
                z_merger.get_list(),
            )
        }
    }

    fn merge_all(
        first: &VoxelShape,
        second: &VoxelShape,
        first_only_matters: bool,
        second_only_matters: bool,
    ) -> (IndexMerger, IndexMerger, IndexMerger) {
        let x = Self::create_index_merger(
            1,
            first.get_coords(Axis::X),
            second.get_coords(Axis::X),
            first_only_matters,
            second_only_matters,
        );
        let y = Self::create_index_merger(
            x.size() as i32 - 1,
            first.get_coords(Axis::Y),
            second.get_coords(Axis::Y),
            first_only_matters,
            second_only_matters,
        );
        let z = Self::create_index_merger(
            (x.size() as i32 - 1).wrapping_mul(y.size() as i32 - 1),
            first.get_coords(Axis::Z),
            second.get_coords(Axis::Z),
            first_only_matters,
            second_only_matters,
        );
        (x, y, z)
    }

    pub fn join_is_not_empty(first: &VoxelShape, second: &VoxelShape, op: BooleanOp) -> bool {
        assert!(
            !op.apply(false, false),
            "op.apply(false, false) must be false"
        );
        let first_empty = first.is_empty();
        let second_empty = second.is_empty();
        if first_empty || second_empty {
            return op.apply(!first_empty, !second_empty);
        }
        if first.ptr_eq(second) {
            return op.apply(true, true);
        }
        let first_only_matters = op.apply(true, false);
        let second_only_matters = op.apply(false, true);
        for axis in Axis::VALUES {
            if first.max(axis) < second.min(axis) - 1.0E-7 {
                return first_only_matters || second_only_matters;
            }
            if second.max(axis) < first.min(axis) - 1.0E-7 {
                return first_only_matters || second_only_matters;
            }
        }
        let (x_merger, y_merger, z_merger) =
            Self::merge_all(first, second, first_only_matters, second_only_matters);
        let (first, second) = (first.shape(), second.shape());
        !x_merger.for_merged_indexes(&mut |x1, x2, _| {
            y_merger.for_merged_indexes(&mut |y1, y2, _| {
                z_merger.for_merged_indexes(&mut |z1, z2, _| {
                    !op.apply(
                        first.is_full_wide(x1, y1, z1),
                        second.is_full_wide(x2, y2, z2),
                    )
                })
            })
        })
    }

    pub fn collide<'a>(
        axis: Axis,
        moving: &AABB,
        shapes: impl IntoIterator<Item = &'a VoxelShape>,
        mut distance: f64,
    ) -> f64 {
        for shape in shapes {
            if distance.abs() < 1.0E-7 {
                return 0.0;
            }
            distance = shape.collide(axis, moving, distance);
        }
        distance
    }

    pub fn block_occludes(shape: &VoxelShape, occluder: &VoxelShape, direction: Direction) -> bool {
        if shape.ptr_eq(&BLOCK) && occluder.ptr_eq(&BLOCK) {
            return true;
        }
        if occluder.is_empty() {
            return false;
        }
        let axis = direction.get_axis();
        let positive = direction.get_axis_direction() == AxisDirection::Positive;
        let first = if positive { shape } else { occluder };
        let second = if positive { occluder } else { shape };
        let op = if positive {
            BooleanOp::ONLY_FIRST
        } else {
            BooleanOp::ONLY_SECOND
        };
        fuzzy_equals(first.max(axis), 1.0, 1.0E-7)
            && fuzzy_equals(second.min(axis), 0.0, 1.0E-7)
            && !Self::join_is_not_empty(
                &first.slice(axis, first.shape().get_size(axis) - 1),
                &second.slice(axis, 0),
                op,
            )
    }

    pub fn merged_face_occludes(
        shape: &VoxelShape,
        occluder: &VoxelShape,
        direction: Direction,
    ) -> bool {
        if shape.ptr_eq(&BLOCK) || occluder.ptr_eq(&BLOCK) {
            return true;
        }
        let axis = direction.get_axis();
        let positive = direction.get_axis_direction() == AxisDirection::Positive;
        let mut first = if positive {
            shape.clone()
        } else {
            occluder.clone()
        };
        let mut second = if positive {
            occluder.clone()
        } else {
            shape.clone()
        };
        if !fuzzy_equals(first.max(axis), 1.0, 1.0E-7) {
            first = Self::empty();
        }
        if !fuzzy_equals(second.min(axis), 0.0, 1.0E-7) {
            second = Self::empty();
        }
        !Self::join_is_not_empty(
            &BLOCK,
            &Self::join_unoptimized(
                &first.slice(axis, first.shape().get_size(axis) - 1),
                &second.slice(axis, 0),
                BooleanOp::OR,
            ),
            BooleanOp::ONLY_FIRST,
        )
    }

    pub fn face_shape_occludes(shape: &VoxelShape, occluder: &VoxelShape) -> bool {
        if shape.ptr_eq(&BLOCK) || occluder.ptr_eq(&BLOCK) {
            true
        } else if shape.is_empty() && occluder.is_empty() {
            false
        } else {
            !Self::join_is_not_empty(
                &BLOCK,
                &Self::join_unoptimized(shape, occluder, BooleanOp::OR),
                BooleanOp::ONLY_FIRST,
            )
        }
    }

    pub fn create_index_merger(
        cost: i32,
        first: &Coords,
        second: &Coords,
        first_only_matters: bool,
        second_only_matters: bool,
    ) -> IndexMerger {
        let first_size = first.size() as i32 - 1;
        let second_size = second.size() as i32 - 1;
        if first.is_cube_point_range() && second.is_cube_point_range() {
            let size = lcm(first_size, second_size);
            if cost as i64 * size <= 256 {
                return IndexMerger::discrete_cube(first_size, second_size);
            }
        }

        if first.get_double(first_size as usize) < second.get_double(0) - 1.0E-7 {
            IndexMerger::NonOverlapping {
                lower: first.clone(),
                upper: second.clone(),
                swap: false,
            }
        } else if second.get_double(second_size as usize) < first.get_double(0) - 1.0E-7 {
            IndexMerger::NonOverlapping {
                lower: second.clone(),
                upper: first.clone(),
                swap: true,
            }
        } else if first_size == second_size && first.list_equals(second) {
            IndexMerger::Identical(first.clone())
        } else {
            IndexMerger::indirect(first, second, first_only_matters, second_only_matters)
        }
    }

    pub fn flip_axis_if_needed(
        new_axis: &Coords,
        flip: bool,
        new_relative: f64,
        old_relative: f64,
    ) -> Coords {
        if !flip && new_relative == old_relative {
            return new_axis.clone();
        }
        let size = new_axis.size();
        if flip {
            Coords::array(
                (0..size)
                    .rev()
                    .map(|i| -(new_axis.get_double(i) - new_relative) + old_relative)
                    .collect::<Vec<_>>(),
            )
        } else {
            Coords::array(
                (0..size)
                    .map(|i| new_axis.get_double(i) - new_relative + old_relative)
                    .collect::<Vec<_>>(),
            )
        }
    }

    pub fn equal(first: &VoxelShape, second: &VoxelShape) -> bool {
        !Self::join_is_not_empty(first, second, BooleanOp::NOT_SAME)
    }

    /// `rotate(shape, rotation)` about [`BLOCK_CENTER`].
    pub fn rotate(shape: &VoxelShape, rotation: OctahedralGroup) -> VoxelShape {
        Self::rotate_with_point(shape, rotation, &BLOCK_CENTER)
    }

    /// `rotate(shape, rotation, rotationPoint)`
    pub fn rotate_with_point(
        shape: &VoxelShape,
        rotation: OctahedralGroup,
        rotation_point: &Vec3,
    ) -> VoxelShape {
        if rotation == OctahedralGroup::Identity {
            return shape.clone();
        }
        let new_discrete_shape = shape.shape().rotate(rotation);
        if shape.is_cube_voxel_shape() && BLOCK_CENTER == *rotation_point {
            return VoxelShape::cube(new_discrete_shape);
        }
        let [new_x, new_y, new_z] = Axis::VALUES.map(|a| rotation.permutation().permute_axis(a));
        let [flip_x, flip_y, flip_z] = Axis::VALUES.map(|a| rotation.inverts(a));
        VoxelShape::array(
            new_discrete_shape,
            Self::flip_axis_if_needed(
                shape.get_coords(new_x),
                flip_x,
                rotation_point.get(new_x),
                rotation_point.x,
            ),
            Self::flip_axis_if_needed(
                shape.get_coords(new_y),
                flip_y,
                rotation_point.get(new_y),
                rotation_point.y,
            ),
            Self::flip_axis_if_needed(
                shape.get_coords(new_z),
                flip_z,
                rotation_point.get(new_z),
                rotation_point.z,
            ),
        )
    }

    /// `rotateHorizontalAxis(zAxis)`
    pub fn rotate_horizontal_axis(z_axis: &VoxelShape) -> HashMap<Axis, VoxelShape> {
        Self::rotate_horizontal_axis_with_center(z_axis, &BLOCK_CENTER)
    }

    /// `rotateHorizontalAxis(zAxis, rotationCenter)`
    pub fn rotate_horizontal_axis_with_center(
        z_axis: &VoxelShape,
        rotation_center: &Vec3,
    ) -> HashMap<Axis, VoxelShape> {
        HashMap::from([
            (Axis::Z, z_axis.clone()),
            (
                Axis::X,
                Self::rotate_with_point(z_axis, OctahedralGroup::BLOCK_ROT_Y_90, rotation_center),
            ),
        ])
    }

    /// `rotateAllAxis(north)`
    pub fn rotate_all_axis(north: &VoxelShape) -> HashMap<Axis, VoxelShape> {
        Self::rotate_all_axis_with_center(north, &BLOCK_CENTER)
    }

    /// `rotateAllAxis(north, rotationCenter)`
    pub fn rotate_all_axis_with_center(
        north: &VoxelShape,
        rotation_center: &Vec3,
    ) -> HashMap<Axis, VoxelShape> {
        HashMap::from([
            (Axis::Z, north.clone()),
            (
                Axis::X,
                Self::rotate_with_point(north, OctahedralGroup::BLOCK_ROT_Y_90, rotation_center),
            ),
            (
                Axis::Y,
                Self::rotate_with_point(north, OctahedralGroup::BLOCK_ROT_X_90, rotation_center),
            ),
        ])
    }

    /// `rotateHorizontal(north)`
    pub fn rotate_horizontal(north: &VoxelShape) -> HashMap<Direction, VoxelShape> {
        Self::rotate_horizontal_with(north, OctahedralGroup::Identity, &BLOCK_CENTER)
    }

    /// `rotateHorizontal(north, initial)`
    pub fn rotate_horizontal_initial(
        north: &VoxelShape,
        initial: OctahedralGroup,
    ) -> HashMap<Direction, VoxelShape> {
        Self::rotate_horizontal_with(north, initial, &BLOCK_CENTER)
    }

    /// `rotateHorizontal(north, initial, rotationCenter)`
    pub fn rotate_horizontal_with(
        north: &VoxelShape,
        initial: OctahedralGroup,
        rotation_center: &Vec3,
    ) -> HashMap<Direction, VoxelShape> {
        HashMap::from([
            // vanilla's NORTH entry rotates about BLOCK_CENTER, ignoring `rotationCenter`
            (Direction::North, Self::rotate(north, initial)),
            (
                Direction::East,
                Self::rotate_with_point(
                    north,
                    OctahedralGroup::BLOCK_ROT_Y_90.compose(initial),
                    rotation_center,
                ),
            ),
            (
                Direction::South,
                Self::rotate_with_point(
                    north,
                    OctahedralGroup::BLOCK_ROT_Y_180.compose(initial),
                    rotation_center,
                ),
            ),
            (
                Direction::West,
                Self::rotate_with_point(
                    north,
                    OctahedralGroup::BLOCK_ROT_Y_270.compose(initial),
                    rotation_center,
                ),
            ),
        ])
    }

    /// `rotateAll(north)`
    pub fn rotate_all(north: &VoxelShape) -> HashMap<Direction, VoxelShape> {
        Self::rotate_all_with(north, OctahedralGroup::Identity, &BLOCK_CENTER)
    }

    /// `rotateAll(north, rotationCenter)`
    pub fn rotate_all_with_center(
        north: &VoxelShape,
        rotation_center: &Vec3,
    ) -> HashMap<Direction, VoxelShape> {
        Self::rotate_all_with(north, OctahedralGroup::Identity, rotation_center)
    }

    /// `rotateAll(north, initial, rotationCenter)`
    pub fn rotate_all_with(
        north: &VoxelShape,
        initial: OctahedralGroup,
        rotation_center: &Vec3,
    ) -> HashMap<Direction, VoxelShape> {
        HashMap::from([
            // vanilla's NORTH entry rotates about BLOCK_CENTER, ignoring `rotationCenter`
            (Direction::North, Self::rotate(north, initial)),
            (
                Direction::East,
                Self::rotate_with_point(
                    north,
                    OctahedralGroup::BLOCK_ROT_Y_90.compose(initial),
                    rotation_center,
                ),
            ),
            (
                Direction::South,
                Self::rotate_with_point(
                    north,
                    OctahedralGroup::BLOCK_ROT_Y_180.compose(initial),
                    rotation_center,
                ),
            ),
            (
                Direction::West,
                Self::rotate_with_point(
                    north,
                    OctahedralGroup::BLOCK_ROT_Y_270.compose(initial),
                    rotation_center,
                ),
            ),
            (
                Direction::Up,
                Self::rotate_with_point(
                    north,
                    OctahedralGroup::BLOCK_ROT_X_270.compose(initial),
                    rotation_center,
                ),
            ),
            (
                Direction::Down,
                Self::rotate_with_point(
                    north,
                    OctahedralGroup::BLOCK_ROT_X_90.compose(initial),
                    rotation_center,
                ),
            ),
        ])
    }

    /// `rotateAttachFace(north)`
    pub fn rotate_attach_face(
        north: &VoxelShape,
    ) -> HashMap<AttachFace, HashMap<Direction, VoxelShape>> {
        Self::rotate_attach_face_initial(north, OctahedralGroup::Identity)
    }

    /// `rotateAttachFace(north, initial)`
    pub fn rotate_attach_face_initial(
        north: &VoxelShape,
        initial: OctahedralGroup,
    ) -> HashMap<AttachFace, HashMap<Direction, VoxelShape>> {
        HashMap::from([
            (
                AttachFace::Wall,
                Self::rotate_horizontal_initial(north, initial),
            ),
            (
                AttachFace::Floor,
                Self::rotate_horizontal_initial(
                    north,
                    OctahedralGroup::BLOCK_ROT_X_270.compose(initial),
                ),
            ),
            (
                AttachFace::Ceiling,
                Self::rotate_horizontal_initial(
                    north,
                    OctahedralGroup::BLOCK_ROT_Y_180
                        .compose(OctahedralGroup::BLOCK_ROT_X_90)
                        .compose(initial),
                ),
            ),
        ])
    }
}
