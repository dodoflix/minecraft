//! `net.minecraft.core.Direction` (with `Axis`, `AxisDirection`, `Plane`), `AxisCycle`, `Directional`,
//! `CompositeDirection`, `CompositeDirection.Direction8` and `FrontAndTop`.
//!
//! Not ported (joml / `Entity` dependent): `Direction.getRotation()` (Quaternionf), `Direction.rotate(Matrix4fc, _)`,
//! `Direction.step()` / `getUnitVec3f()` (Vector3f). `orderedByNearest` and `getFacingAxis` take the entity's view
//! rotations instead of the entity. Codec/`StreamCodec` members belong to the codec crates (`by_name`, `by_id` are here).

use crate::math::mth;
use crate::phys::Vec3;
use crate::pos::Vec3i;
use crate::random::RandomSource;
use std::fmt;

/// `(float)(Math.PI / 180.0)`
const DEG_TO_RAD_F32: f32 = (std::f64::consts::PI / 180.0) as f32;

/// `Util.shuffle` applied to a copy (`Util.shuffledCopy`).
pub fn shuffled_copy<T: Copy>(values: &[T], random: &mut dyn RandomSource) -> Vec<T> {
    let mut copy = values.to_vec();
    let mut i = copy.len();
    while i > 1 {
        let swap_to = random.next_int_bounded(i as i32) as usize;
        copy.swap(i - 1, swap_to);
        i -= 1;
    }
    copy
}

/// `core.Directional`
pub trait Directional {
    fn get_step_x(&self) -> i32;
    fn get_step_y(&self) -> i32;
    fn get_step_z(&self) -> i32;
    fn get_step(&self) -> Vec3i;
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Direction {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

use Direction::{Down, East, North, South, Up, West};

impl Direction {
    /// All directions in ordinal (= 3D data value) order.
    pub const ALL: [Direction; 6] = [Down, Up, North, South, West, East];
    /// Same as [`Direction::ALL`] (vanilla `VALUES`).
    pub const VALUES: [Direction; 6] = Self::ALL;
    /// `Plane.HORIZONTAL` faces.
    pub const HORIZONTAL: [Direction; 4] = [North, East, South, West];
    /// `Plane.VERTICAL` faces.
    pub const VERTICAL: [Direction; 2] = [Up, Down];

    const BY_2D_DATA: [Direction; 4] = [South, West, North, East];

    pub const fn ordinal(self) -> usize {
        self as usize
    }

    pub const fn get_3d_data_value(self) -> usize {
        self as usize
    }

    pub const fn get_2d_data_value(self) -> i32 {
        match self {
            Down | Up => -1,
            North => 2,
            South => 0,
            West => 1,
            East => 3,
        }
    }

    const fn opposite_index(self) -> usize {
        (self as usize) ^ 1
    }

    pub const fn get_axis_direction(self) -> AxisDirection {
        match self {
            Down | North | West => AxisDirection::Negative,
            Up | South | East => AxisDirection::Positive,
        }
    }

    pub const fn get_axis(self) -> Axis {
        match self {
            Down | Up => Axis::Y,
            North | South => Axis::Z,
            West | East => Axis::X,
        }
    }

    pub const fn get_name(self) -> &'static str {
        match self {
            Down => "down",
            Up => "up",
            North => "north",
            South => "south",
            West => "west",
            East => "east",
        }
    }

    /// `getSerializedName`
    pub const fn get_serialized_name(self) -> &'static str {
        self.get_name()
    }

    pub const fn get_opposite(self) -> Direction {
        Self::ALL[self.opposite_index()]
    }

    pub const fn get_step_x(self) -> i32 {
        match self {
            West => -1,
            East => 1,
            _ => 0,
        }
    }

    pub const fn get_step_y(self) -> i32 {
        match self {
            Down => -1,
            Up => 1,
            _ => 0,
        }
    }

    pub const fn get_step_z(self) -> i32 {
        match self {
            North => -1,
            South => 1,
            _ => 0,
        }
    }

    /// `getUnitVec3i` (also `getStep`)
    pub fn get_unit_vec3i(self) -> Vec3i {
        Vec3i::new(self.get_step_x(), self.get_step_y(), self.get_step_z())
    }

    /// `getStep`
    pub fn get_step(self) -> Vec3i {
        self.get_unit_vec3i()
    }

    /// `getUnitVec3`
    pub fn get_unit_vec3(self) -> Vec3 {
        Vec3::new(self.get_step_x() as f64, self.get_step_y() as f64, self.get_step_z() as f64)
    }

    /// `Direction.byName` (exact, case-sensitive match of the serialized name).
    pub fn by_name(name: &str) -> Option<Direction> {
        Self::ALL.into_iter().find(|d| d.get_name() == name)
    }

    /// `BY_ID` (`ByIdMap.continuous(.., WRAP)`)
    pub fn by_id(id: i32) -> Direction {
        Self::ALL[mth::positive_modulo_i32(id, 6) as usize]
    }

    /// `from3DDataValue`: `BY_3D_DATA[Mth.abs(data % length)]`
    pub fn from_3d_data_value(data: i32) -> Direction {
        Self::ALL[(data % 6).unsigned_abs() as usize]
    }

    /// `from2DDataValue`
    pub fn from_2d_data_value(data: i32) -> Direction {
        Self::BY_2D_DATA[(data % 4).unsigned_abs() as usize]
    }

    /// `fromYRot`
    pub fn from_y_rot(y_rot: f64) -> Direction {
        Self::from_2d_data_value(mth::floor_f64(y_rot / 90.0 + 0.5) & 3)
    }

    pub const fn from_axis_and_direction(axis: Axis, direction: AxisDirection) -> Direction {
        let positive = matches!(direction, AxisDirection::Positive);
        match axis {
            Axis::X => {
                if positive {
                    East
                } else {
                    West
                }
            }
            Axis::Y => {
                if positive {
                    Up
                } else {
                    Down
                }
            }
            Axis::Z => {
                if positive {
                    South
                } else {
                    North
                }
            }
        }
    }

    /// `Direction.get(AxisDirection, Axis)`
    pub const fn get(axis_direction: AxisDirection, axis: Axis) -> Direction {
        Self::from_axis_and_direction(axis, axis_direction)
    }

    /// `toYRot`
    pub const fn to_y_rot(self) -> f32 {
        ((self.get_2d_data_value() & 3) * 90) as f32
    }

    /// `Direction.getYRot(Direction)`; panics for vertical directions like vanilla's `IllegalStateException`.
    pub fn get_y_rot(direction: Direction) -> f32 {
        match direction {
            North => 180.0,
            South => 0.0,
            West => 90.0,
            East => -90.0,
            _ => panic!("No y-Rot for vertical axis: {direction}"),
        }
    }

    /// `Direction.getRandom`
    pub fn get_random(random: &mut dyn RandomSource) -> Direction {
        Self::ALL[random.next_int_bounded(6) as usize]
    }

    /// `Direction.allShuffled`
    pub fn all_shuffled(random: &mut dyn RandomSource) -> Vec<Direction> {
        shuffled_copy(&Self::ALL, random)
    }

    /// `getClockWise(Axis)`
    pub fn get_clockwise_axis(self, axis: Axis) -> Direction {
        match axis {
            Axis::X => {
                if self != West && self != East {
                    self.get_clockwise_x()
                } else {
                    self
                }
            }
            Axis::Y => {
                if self != Up && self != Down {
                    self.get_clockwise()
                } else {
                    self
                }
            }
            Axis::Z => {
                if self != North && self != South {
                    self.get_clockwise_z()
                } else {
                    self
                }
            }
        }
    }

    /// `getCounterClockWise(Axis)`
    pub fn get_counter_clockwise_axis(self, axis: Axis) -> Direction {
        match axis {
            Axis::X => {
                if self != West && self != East {
                    self.get_counter_clockwise_x()
                } else {
                    self
                }
            }
            Axis::Y => {
                if self != Up && self != Down {
                    self.get_counter_clockwise()
                } else {
                    self
                }
            }
            Axis::Z => {
                if self != North && self != South {
                    self.get_counter_clockwise_z()
                } else {
                    self
                }
            }
        }
    }

    /// `getClockWise`; panics for vertical directions like vanilla's `IllegalStateException`.
    pub fn get_clockwise(self) -> Direction {
        match self {
            North => East,
            South => West,
            West => North,
            East => South,
            _ => panic!("Unable to get Y-rotated facing of {self}"),
        }
    }

    fn get_clockwise_x(self) -> Direction {
        match self {
            Down => South,
            Up => North,
            North => Down,
            South => Up,
            _ => panic!("Unable to get X-rotated facing of {self}"),
        }
    }

    fn get_counter_clockwise_x(self) -> Direction {
        match self {
            Down => North,
            Up => South,
            North => Up,
            South => Down,
            _ => panic!("Unable to get X-rotated facing of {self}"),
        }
    }

    fn get_clockwise_z(self) -> Direction {
        match self {
            Down => West,
            Up => East,
            West => Up,
            East => Down,
            _ => panic!("Unable to get Z-rotated facing of {self}"),
        }
    }

    fn get_counter_clockwise_z(self) -> Direction {
        match self {
            Down => East,
            Up => West,
            West => Down,
            East => Up,
            _ => panic!("Unable to get Z-rotated facing of {self}"),
        }
    }

    /// `getCounterClockWise`; panics for vertical directions like vanilla's `IllegalStateException`.
    pub fn get_counter_clockwise(self) -> Direction {
        match self {
            North => West,
            South => East,
            West => South,
            East => North,
            _ => panic!("Unable to get CCW facing of {self}"),
        }
    }

    /// `getApproximateNearest(float, float, float)`
    pub fn get_approximate_nearest_f32(dx: f32, dy: f32, dz: f32) -> Direction {
        let mut result = North;
        let mut highest_dot = f32::from_bits(1); // Float.MIN_VALUE
        for direction in Self::ALL {
            let dot = dx * direction.get_step_x() as f32
                + dy * direction.get_step_y() as f32
                + dz * direction.get_step_z() as f32;
            if dot > highest_dot {
                highest_dot = dot;
                result = direction;
            }
        }
        result
    }

    /// `getApproximateNearest(double, double, double)`
    pub fn get_approximate_nearest(dx: f64, dy: f64, dz: f64) -> Direction {
        Self::get_approximate_nearest_f32(dx as f32, dy as f32, dz as f32)
    }

    /// `getApproximateNearest(Vec3)`
    pub fn get_approximate_nearest_vec3(vec: Vec3) -> Direction {
        Self::get_approximate_nearest(vec.x, vec.y, vec.z)
    }

    /// `getNearest(int, int, int, orElse)`
    pub fn get_nearest(x: i32, y: i32, z: i32, or_else: Option<Direction>) -> Option<Direction> {
        let abs_x = x.wrapping_abs();
        let abs_y = y.wrapping_abs();
        let abs_z = z.wrapping_abs();
        if abs_x > abs_z && abs_x > abs_y {
            Some(if x < 0 { West } else { East })
        } else if abs_z > abs_x && abs_z > abs_y {
            Some(if z < 0 { North } else { South })
        } else if abs_y > abs_x && abs_y > abs_z {
            Some(if y < 0 { Down } else { Up })
        } else {
            or_else
        }
    }

    /// `getNearest(Vec3i, orElse)`
    pub fn get_nearest_vec3i(vec: Vec3i, or_else: Option<Direction>) -> Option<Direction> {
        Self::get_nearest(vec.x(), vec.y(), vec.z(), or_else)
    }

    /// `axisStepOrder`
    pub fn axis_step_order(movement: Vec3) -> [Axis; 3] {
        if movement.x.abs() < movement.z.abs() {
            [Axis::Y, Axis::Z, Axis::X]
        } else {
            [Axis::Y, Axis::X, Axis::Z]
        }
    }

    /// `isFacingAngle`
    pub fn is_facing_angle(self, y_angle: f32) -> bool {
        let radians = y_angle * DEG_TO_RAD_F32;
        let dx = -mth::sin(radians as f64);
        let dz = mth::cos(radians as f64);
        self.get_step_x() as f32 * dx + self.get_step_z() as f32 * dz > 0.0
    }

    /// `getFacingAxis(entity, axis)` with the entity's `getViewXRot(1.0F)` / `getViewYRot(1.0F)`.
    pub fn get_facing_axis(axis: Axis, view_x_rot: f32, view_y_rot: f32) -> Direction {
        match axis {
            Axis::X => {
                if East.is_facing_angle(view_y_rot) {
                    East
                } else {
                    West
                }
            }
            Axis::Y => {
                if view_x_rot < 0.0 {
                    Up
                } else {
                    Down
                }
            }
            Axis::Z => {
                if South.is_facing_angle(view_y_rot) {
                    South
                } else {
                    North
                }
            }
        }
    }

    /// `orderedByNearest(entity)` with the entity's `getViewXRot(1.0F)` / `getViewYRot(1.0F)`.
    pub fn ordered_by_nearest(view_x_rot: f32, view_y_rot: f32) -> [Direction; 6] {
        let pitch = view_x_rot * DEG_TO_RAD_F32;
        let yaw = -view_y_rot * DEG_TO_RAD_F32;
        let pitch_sin = mth::sin(pitch as f64);
        let pitch_cos = mth::cos(pitch as f64);
        let yaw_sin = mth::sin(yaw as f64);
        let yaw_cos = mth::cos(yaw as f64);
        let x_pos = yaw_sin > 0.0;
        let y_pos = pitch_sin < 0.0;
        let z_pos = yaw_cos > 0.0;
        let x_yaw = if x_pos { yaw_sin } else { -yaw_sin };
        let y_mag = if y_pos { -pitch_sin } else { pitch_sin };
        let z_yaw = if z_pos { yaw_cos } else { -yaw_cos };
        let x_mag = x_yaw * pitch_cos;
        let z_mag = z_yaw * pitch_cos;
        let axis_x = if x_pos { East } else { West };
        let axis_y = if y_pos { Up } else { Down };
        let axis_z = if z_pos { South } else { North };
        if x_yaw > z_yaw {
            if y_mag > x_mag {
                Self::make_direction_array(axis_y, axis_x, axis_z)
            } else if z_mag > y_mag {
                Self::make_direction_array(axis_x, axis_z, axis_y)
            } else {
                Self::make_direction_array(axis_x, axis_y, axis_z)
            }
        } else if y_mag > z_mag {
            Self::make_direction_array(axis_y, axis_z, axis_x)
        } else if x_mag > y_mag {
            Self::make_direction_array(axis_z, axis_x, axis_y)
        } else {
            Self::make_direction_array(axis_z, axis_y, axis_x)
        }
    }

    fn make_direction_array(a1: Direction, a2: Direction, a3: Direction) -> [Direction; 6] {
        [a1, a2, a3, a3.get_opposite(), a2.get_opposite(), a1.get_opposite()]
    }
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.get_name())
    }
}

impl Directional for Direction {
    fn get_step_x(&self) -> i32 {
        Direction::get_step_x(*self)
    }
    fn get_step_y(&self) -> i32 {
        Direction::get_step_y(*self)
    }
    fn get_step_z(&self) -> i32 {
        Direction::get_step_z(*self)
    }
    fn get_step(&self) -> Vec3i {
        self.get_unit_vec3i()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub const ALL: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];
    /// Same as [`Axis::ALL`] (vanilla `VALUES`).
    pub const VALUES: [Axis; 3] = Self::ALL;

    pub const fn ordinal(self) -> usize {
        self as usize
    }

    pub fn by_name(name: &str) -> Option<Axis> {
        Self::ALL.into_iter().find(|a| a.get_name() == name)
    }

    pub const fn get_name(self) -> &'static str {
        match self {
            Axis::X => "x",
            Axis::Y => "y",
            Axis::Z => "z",
        }
    }

    pub const fn get_serialized_name(self) -> &'static str {
        self.get_name()
    }

    pub const fn is_vertical(self) -> bool {
        matches!(self, Axis::Y)
    }

    pub const fn is_horizontal(self) -> bool {
        matches!(self, Axis::X | Axis::Z)
    }

    pub const fn get_positive(self) -> Direction {
        match self {
            Axis::X => East,
            Axis::Y => Up,
            Axis::Z => South,
        }
    }

    pub const fn get_negative(self) -> Direction {
        match self {
            Axis::X => West,
            Axis::Y => Down,
            Axis::Z => North,
        }
    }

    pub const fn get_directions(self) -> [Direction; 2] {
        [self.get_positive(), self.get_negative()]
    }

    pub fn get_random(random: &mut dyn RandomSource) -> Axis {
        Self::ALL[random.next_int_bounded(3) as usize]
    }

    /// `Predicate<Direction>.test`
    pub fn test(self, input: Direction) -> bool {
        input.get_axis() == self
    }

    pub const fn get_plane(self) -> Plane {
        match self {
            Axis::X | Axis::Z => Plane::Horizontal,
            Axis::Y => Plane::Vertical,
        }
    }

    /// `choose(int|double|boolean ...)`
    pub const fn choose<T: Copy>(self, x: T, y: T, z: T) -> T {
        match self {
            Axis::X => x,
            Axis::Y => y,
            Axis::Z => z,
        }
    }
}

impl fmt::Display for Axis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.get_name())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AxisDirection {
    Positive,
    Negative,
}

impl AxisDirection {
    pub const fn get_step(self) -> i32 {
        match self {
            AxisDirection::Positive => 1,
            AxisDirection::Negative => -1,
        }
    }

    pub const fn get_name(self) -> &'static str {
        match self {
            AxisDirection::Positive => "Towards positive",
            AxisDirection::Negative => "Towards negative",
        }
    }

    pub const fn opposite(self) -> AxisDirection {
        match self {
            AxisDirection::Positive => AxisDirection::Negative,
            AxisDirection::Negative => AxisDirection::Positive,
        }
    }
}

impl fmt::Display for AxisDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.get_name())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Plane {
    Horizontal,
    Vertical,
}

impl Plane {
    const HORIZONTAL_AXES: [Axis; 2] = [Axis::X, Axis::Z];
    const VERTICAL_AXES: [Axis; 1] = [Axis::Y];

    pub const fn faces(self) -> &'static [Direction] {
        match self {
            Plane::Horizontal => &Direction::HORIZONTAL,
            Plane::Vertical => &Direction::VERTICAL,
        }
    }

    pub const fn axes(self) -> &'static [Axis] {
        match self {
            Plane::Horizontal => &Self::HORIZONTAL_AXES,
            Plane::Vertical => &Self::VERTICAL_AXES,
        }
    }

    pub fn get_random_direction(self, random: &mut dyn RandomSource) -> Direction {
        let faces = self.faces();
        faces[random.next_int_bounded(faces.len() as i32) as usize]
    }

    pub fn get_random_axis(self, random: &mut dyn RandomSource) -> Axis {
        let axes = self.axes();
        axes[random.next_int_bounded(axes.len() as i32) as usize]
    }

    /// `Predicate<Direction>.test`
    pub fn test(self, input: Direction) -> bool {
        input.get_axis().get_plane() == self
    }

    pub fn iter(self) -> std::iter::Copied<std::slice::Iter<'static, Direction>> {
        self.faces().iter().copied()
    }

    pub fn shuffled_copy(self, random: &mut dyn RandomSource) -> Vec<Direction> {
        shuffled_copy(self.faces(), random)
    }

    pub const fn length(self) -> usize {
        self.faces().len()
    }
}

impl IntoIterator for Plane {
    type Item = Direction;
    type IntoIter = std::iter::Copied<std::slice::Iter<'static, Direction>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// `core.AxisCycle`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AxisCycle {
    None,
    Forward,
    Backward,
}

impl AxisCycle {
    pub const VALUES: [AxisCycle; 3] = [AxisCycle::None, AxisCycle::Forward, AxisCycle::Backward];

    pub const fn cycle<T: Copy>(self, x: T, y: T, z: T, axis: Axis) -> T {
        match self {
            AxisCycle::None => axis.choose(x, y, z),
            AxisCycle::Forward => axis.choose(z, x, y),
            AxisCycle::Backward => axis.choose(y, z, x),
        }
    }

    pub const fn cycle_i32(self, x: i32, y: i32, z: i32, axis: Axis) -> i32 {
        self.cycle(x, y, z, axis)
    }

    pub const fn cycle_f64(self, x: f64, y: f64, z: f64, axis: Axis) -> f64 {
        self.cycle(x, y, z, axis)
    }

    pub const fn cycle_axis(self, axis: Axis) -> Axis {
        match self {
            AxisCycle::None => axis,
            AxisCycle::Forward => Axis::ALL[(axis as usize + 1) % 3],
            AxisCycle::Backward => Axis::ALL[(axis as usize + 2) % 3],
        }
    }

    pub const fn inverse(self) -> AxisCycle {
        match self {
            AxisCycle::None => AxisCycle::None,
            AxisCycle::Forward => AxisCycle::Backward,
            AxisCycle::Backward => AxisCycle::Forward,
        }
    }

    pub const fn between(from: Axis, to: Axis) -> AxisCycle {
        Self::VALUES[(to as usize + 3 - from as usize) % 3]
    }
}

/// `core.CompositeDirection`: a set of directions (bitmask over `Direction` ordinals) and its summed step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CompositeDirection {
    directions: u8,
    step: [i32; 3],
}

impl CompositeDirection {
    pub const NORTH: Self = Self::of(North);
    pub const EAST: Self = Self::of(East);
    pub const SOUTH: Self = Self::of(South);
    pub const WEST: Self = Self::of(West);
    pub const NORTH_EAST: Self = Self::NORTH.compose(Self::EAST);
    pub const SOUTH_EAST: Self = Self::SOUTH.compose(Self::EAST);
    pub const SOUTH_WEST: Self = Self::SOUTH.compose(Self::WEST);
    pub const NORTH_WEST: Self = Self::NORTH.compose(Self::WEST);
    pub const UP: Self = Self::of(Up);
    pub const NORTH_UP: Self = Self::NORTH.compose(Self::UP);
    pub const EAST_UP: Self = Self::EAST.compose(Self::UP);
    pub const SOUTH_UP: Self = Self::SOUTH.compose(Self::UP);
    pub const WEST_UP: Self = Self::WEST.compose(Self::UP);
    pub const NORTH_EAST_UP: Self = Self::NORTH_EAST.compose(Self::UP);
    pub const SOUTH_EAST_UP: Self = Self::SOUTH_EAST.compose(Self::UP);
    pub const SOUTH_WEST_UP: Self = Self::SOUTH_WEST.compose(Self::UP);
    pub const NORTH_WEST_UP: Self = Self::NORTH_WEST.compose(Self::UP);
    pub const DOWN: Self = Self::of(Down);
    pub const NORTH_DOWN: Self = Self::NORTH.compose(Self::DOWN);
    pub const EAST_DOWN: Self = Self::EAST.compose(Self::DOWN);
    pub const SOUTH_DOWN: Self = Self::SOUTH.compose(Self::DOWN);
    pub const WEST_DOWN: Self = Self::WEST.compose(Self::DOWN);
    pub const NORTH_EAST_DOWN: Self = Self::NORTH_EAST.compose(Self::DOWN);
    pub const SOUTH_EAST_DOWN: Self = Self::SOUTH_EAST.compose(Self::DOWN);
    pub const SOUTH_WEST_DOWN: Self = Self::SOUTH_WEST.compose(Self::DOWN);
    pub const NORTH_WEST_DOWN: Self = Self::NORTH_WEST.compose(Self::DOWN);

    const fn of(direction: Direction) -> Self {
        Self {
            directions: 1 << direction as u8,
            step: [direction.get_step_x(), direction.get_step_y(), direction.get_step_z()],
        }
    }

    /// The `CompositeDirection(Direction...)` constructor (duplicates collapse like an `EnumSet`).
    /// Panics on an empty list like vanilla's `IllegalArgumentException`.
    pub fn new(directions: &[Direction]) -> Self {
        let mut result: Option<Self> = None;
        let mut seen = 0u8;
        for d in directions {
            if seen & (1 << *d as u8) == 0 {
                seen |= 1 << *d as u8;
                result = Some(match result {
                    Some(r) => r.compose(Self::of(*d)),
                    None => Self::of(*d),
                });
            }
        }
        result.expect("Directions cannot be empty")
    }

    /// The canonical record constructor `(Set<Direction>, Vec3i step)`.
    pub fn from_parts(directions: &[Direction], step: Vec3i) -> Result<Self, &'static str> {
        if directions.is_empty() {
            return Err("Directions cannot be empty");
        }
        let mask = directions.iter().fold(0u8, |m, d| m | 1 << *d as u8);
        Ok(Self { directions: mask, step: [step.x(), step.y(), step.z()] })
    }

    /// Plain union of the sets and sum of the steps (note: `Set.addAll`, not a symmetric merge).
    pub const fn compose(self, other: Self) -> Self {
        Self {
            directions: self.directions | other.directions,
            step: [
                self.step[0].wrapping_add(other.step[0]),
                self.step[1].wrapping_add(other.step[1]),
                self.step[2].wrapping_add(other.step[2]),
            ],
        }
    }

    /// `directions()`, in `EnumSet` (ordinal) iteration order.
    pub fn directions(&self) -> Vec<Direction> {
        Direction::ALL.into_iter().filter(|d| self.directions & (1 << *d as u8) != 0).collect()
    }

    pub fn contains(&self, direction: Direction) -> bool {
        self.directions & (1 << direction as u8) != 0
    }
}

impl Directional for CompositeDirection {
    fn get_step_x(&self) -> i32 {
        self.step[0]
    }
    fn get_step_y(&self) -> i32 {
        self.step[1]
    }
    fn get_step_z(&self) -> i32 {
        self.step[2]
    }
    fn get_step(&self) -> Vec3i {
        Vec3i::new(self.step[0], self.step[1], self.step[2])
    }
}

/// `CompositeDirection.Direction8`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Direction8 {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
}

impl Direction8 {
    pub const ALL: [Direction8; 8] = [
        Direction8::North,
        Direction8::NorthEast,
        Direction8::East,
        Direction8::SouthEast,
        Direction8::South,
        Direction8::SouthWest,
        Direction8::West,
        Direction8::NorthWest,
    ];

    pub const fn composite_direction(self) -> CompositeDirection {
        match self {
            Direction8::North => CompositeDirection::NORTH,
            Direction8::NorthEast => CompositeDirection::NORTH_EAST,
            Direction8::East => CompositeDirection::EAST,
            Direction8::SouthEast => CompositeDirection::SOUTH_EAST,
            Direction8::South => CompositeDirection::SOUTH,
            Direction8::SouthWest => CompositeDirection::SOUTH_WEST,
            Direction8::West => CompositeDirection::WEST,
            Direction8::NorthWest => CompositeDirection::NORTH_WEST,
        }
    }

    pub fn get_directions(self) -> Vec<Direction> {
        self.composite_direction().directions()
    }
}

impl Directional for Direction8 {
    fn get_step_x(&self) -> i32 {
        self.composite_direction().step[0]
    }
    fn get_step_y(&self) -> i32 {
        self.composite_direction().step[1]
    }
    fn get_step_z(&self) -> i32 {
        self.composite_direction().step[2]
    }
    fn get_step(&self) -> Vec3i {
        Directional::get_step(&self.composite_direction())
    }
}

/// `core.FrontAndTop`: the 12 (front, top) orientations of a block such as a jigsaw (ordinal order as in vanilla).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FrontAndTop {
    DownEast,
    DownNorth,
    DownSouth,
    DownWest,
    UpEast,
    UpNorth,
    UpSouth,
    UpWest,
    WestUp,
    EastUp,
    NorthUp,
    SouthUp,
}

impl FrontAndTop {
    pub const ALL: [FrontAndTop; 12] = [
        Self::DownEast,
        Self::DownNorth,
        Self::DownSouth,
        Self::DownWest,
        Self::UpEast,
        Self::UpNorth,
        Self::UpSouth,
        Self::UpWest,
        Self::WestUp,
        Self::EastUp,
        Self::NorthUp,
        Self::SouthUp,
    ];

    pub const fn ordinal(self) -> usize {
        self as usize
    }

    /// `front()`
    pub const fn front(self) -> Direction {
        match self {
            Self::DownEast | Self::DownNorth | Self::DownSouth | Self::DownWest => Down,
            Self::UpEast | Self::UpNorth | Self::UpSouth | Self::UpWest => Up,
            Self::WestUp => West,
            Self::EastUp => East,
            Self::NorthUp => North,
            Self::SouthUp => South,
        }
    }

    /// `top()`
    pub const fn top(self) -> Direction {
        match self {
            Self::DownEast | Self::UpEast => East,
            Self::DownNorth | Self::UpNorth => North,
            Self::DownSouth | Self::UpSouth => South,
            Self::DownWest | Self::UpWest => West,
            Self::WestUp | Self::EastUp | Self::NorthUp | Self::SouthUp => Up,
        }
    }

    pub const fn get_serialized_name(self) -> &'static str {
        match self {
            Self::DownEast => "down_east",
            Self::DownNorth => "down_north",
            Self::DownSouth => "down_south",
            Self::DownWest => "down_west",
            Self::UpEast => "up_east",
            Self::UpNorth => "up_north",
            Self::UpSouth => "up_south",
            Self::UpWest => "up_west",
            Self::WestUp => "west_up",
            Self::EastUp => "east_up",
            Self::NorthUp => "north_up",
            Self::SouthUp => "south_up",
        }
    }

    /// `CODEC` lookup by serialized name.
    pub fn by_name(name: &str) -> Option<FrontAndTop> {
        Self::ALL.into_iter().find(|f| f.get_serialized_name() == name)
    }

    /// `fromFrontAndTop`: `None` where vanilla's lookup table holds `null` (not one of the 12 constants).
    pub fn from_front_and_top(front: Direction, top: Direction) -> Option<FrontAndTop> {
        Self::ALL.into_iter().find(|f| f.front() == front && f.top() == top)
    }
}

impl fmt::Display for FrontAndTop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.get_serialized_name())
    }
}
