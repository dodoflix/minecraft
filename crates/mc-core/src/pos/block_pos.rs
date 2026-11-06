//! `net.minecraft.core.BlockPos` and its nested `MutableBlockPos`.
//!
//! `BlockPos` is an immutable `Copy` newtype over [`Vec3i`]. `MutableBlockPos` wraps a `BlockPos` and
//! dereferences to it, so it can be used anywhere a `&BlockPos` (or `&Vec3i`) is expected. Vanilla's
//! iterators that reuse one `MutableBlockPos` instead yield a fresh `BlockPos` per step.
use super::vec3i::{Vec3i, fmt_named};
use crate::bounding_box::BoundingBox;
use crate::direction::{Axis, AxisCycle, Direction};
use crate::math::mth;
use crate::phys::{AABB, Vec3};
use crate::random::RandomSource;
use crate::rotation::Rotation;
use std::collections::{HashSet, VecDeque};
use std::fmt;
use std::ops::Deref;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct BlockPos(Vec3i);

/// `1 + Mth.log2(Mth.smallestEncompassingPowerOfTwo(30000000))`
pub const PACKED_HORIZONTAL_LENGTH: i32 = 26;
pub const PACKED_Y_LENGTH: i32 = 64 - 2 * PACKED_HORIZONTAL_LENGTH;
const PACKED_X_MASK: i64 = (1i64 << PACKED_HORIZONTAL_LENGTH) - 1;
const PACKED_Y_MASK: i64 = (1i64 << PACKED_Y_LENGTH) - 1;
const PACKED_Z_MASK: i64 = (1i64 << PACKED_HORIZONTAL_LENGTH) - 1;
const Y_OFFSET: i32 = 0;
const Z_OFFSET: i32 = PACKED_Y_LENGTH;
const X_OFFSET: i32 = PACKED_Y_LENGTH + PACKED_HORIZONTAL_LENGTH;
pub const MAX_HORIZONTAL_COORDINATE: i32 = (1 << PACKED_HORIZONTAL_LENGTH) / 2 - 1;

/// `BlockPos.TraversalNodeStatus`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TraversalNodeStatus {
    Accept,
    Skip,
    Stop,
}

impl Deref for BlockPos {
    type Target = Vec3i;

    fn deref(&self) -> &Vec3i {
        &self.0
    }
}

impl From<Vec3i> for BlockPos {
    fn from(v: Vec3i) -> Self {
        BlockPos(v)
    }
}

impl From<BlockPos> for Vec3i {
    fn from(p: BlockPos) -> Self {
        p.0
    }
}

impl fmt::Display for BlockPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_named(f, "BlockPos", self.0.x, self.0.y, self.0.z)
    }
}

impl fmt::Debug for BlockPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// An `AbstractIterator` clone: once `next` returned `None` it keeps doing so.
fn iter_from_fn<T>(f: impl FnMut() -> Option<T>) -> impl Iterator<Item = T> {
    std::iter::from_fn(f).fuse()
}

impl BlockPos {
    pub const ZERO: BlockPos = BlockPos::new(0, 0, 0);

    pub const fn new(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos(Vec3i::new(x, y, z))
    }

    pub const fn x(&self) -> i32 {
        self.0.x
    }

    pub const fn y(&self) -> i32 {
        self.0.y
    }

    pub const fn z(&self) -> i32 {
        self.0.z
    }

    // ----- packed long form -----

    /// `BlockPos.offset(long, Direction)`
    pub fn offset_long_direction(block_node: i64, offset: Direction) -> i64 {
        Self::offset_long(
            block_node,
            offset.get_step_x(),
            offset.get_step_y(),
            offset.get_step_z(),
        )
    }

    /// `BlockPos.offset(long, int, int, int)`
    pub const fn offset_long(block_node: i64, step_x: i32, step_y: i32, step_z: i32) -> i64 {
        Self::as_long_xyz(
            Self::get_x(block_node).wrapping_add(step_x),
            Self::get_y(block_node).wrapping_add(step_y),
            Self::get_z(block_node).wrapping_add(step_z),
        )
    }

    /// `BlockPos.getX(long)`
    pub const fn get_x(block_node: i64) -> i32 {
        (block_node << (64 - X_OFFSET - PACKED_HORIZONTAL_LENGTH)
            >> (64 - PACKED_HORIZONTAL_LENGTH)) as i32
    }

    /// `BlockPos.getY(long)`
    pub const fn get_y(block_node: i64) -> i32 {
        (block_node << (64 - PACKED_Y_LENGTH) >> (64 - PACKED_Y_LENGTH)) as i32
    }

    /// `BlockPos.getZ(long)`
    pub const fn get_z(block_node: i64) -> i32 {
        (block_node << (64 - Z_OFFSET - PACKED_HORIZONTAL_LENGTH)
            >> (64 - PACKED_HORIZONTAL_LENGTH)) as i32
    }

    /// `BlockPos.of(long)`
    pub const fn of_long(block_node: i64) -> BlockPos {
        BlockPos::new(
            Self::get_x(block_node),
            Self::get_y(block_node),
            Self::get_z(block_node),
        )
    }

    pub fn containing(x: f64, y: f64, z: f64) -> BlockPos {
        BlockPos::new(mth::floor_f64(x), mth::floor_f64(y), mth::floor_f64(z))
    }

    /// `BlockPos.containing(Position)`
    pub fn containing_position(pos: &Vec3) -> BlockPos {
        Self::containing(pos.x, pos.y, pos.z)
    }

    pub fn min(a: &BlockPos, b: &BlockPos) -> BlockPos {
        BlockPos::new(a.x().min(b.x()), a.y().min(b.y()), a.z().min(b.z()))
    }

    pub fn max(a: &BlockPos, b: &BlockPos) -> BlockPos {
        BlockPos::new(a.x().max(b.x()), a.y().max(b.y()), a.z().max(b.z()))
    }

    /// `asLong()`
    pub const fn as_long(&self) -> i64 {
        Self::as_long_xyz(self.0.x, self.0.y, self.0.z)
    }

    /// `BlockPos.asLong(int, int, int)`
    pub const fn as_long_xyz(x: i32, y: i32, z: i32) -> i64 {
        let mut node = 0i64;
        node |= (x as i64 & PACKED_X_MASK) << X_OFFSET;
        node |= (y as i64 & PACKED_Y_MASK) << Y_OFFSET;
        node | (z as i64 & PACKED_Z_MASK) << Z_OFFSET
    }

    pub const fn get_flat_index(neighbor_block_node: i64) -> i64 {
        neighbor_block_node & -16
    }

    // ----- arithmetic -----

    pub const fn offset(&self, x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos(self.0.offset(x, y, z))
    }

    pub const fn offset_vec(&self, vec: &Vec3i) -> BlockPos {
        BlockPos(self.0.offset_vec(vec))
    }

    pub const fn subtract(&self, vec: &Vec3i) -> BlockPos {
        BlockPos(self.0.subtract(vec))
    }

    pub const fn multiply(&self, scale: i32) -> BlockPos {
        BlockPos(self.0.multiply(scale))
    }

    pub fn above(&self) -> BlockPos {
        self.relative(Direction::Up)
    }

    pub fn above_n(&self, steps: i32) -> BlockPos {
        self.relative_n(Direction::Up, steps)
    }

    pub fn below(&self) -> BlockPos {
        self.relative(Direction::Down)
    }

    pub fn below_n(&self, steps: i32) -> BlockPos {
        self.relative_n(Direction::Down, steps)
    }

    pub fn north(&self) -> BlockPos {
        self.relative(Direction::North)
    }

    pub fn north_n(&self, steps: i32) -> BlockPos {
        self.relative_n(Direction::North, steps)
    }

    pub fn south(&self) -> BlockPos {
        self.relative(Direction::South)
    }

    pub fn south_n(&self, steps: i32) -> BlockPos {
        self.relative_n(Direction::South, steps)
    }

    pub fn west(&self) -> BlockPos {
        self.relative(Direction::West)
    }

    pub fn west_n(&self, steps: i32) -> BlockPos {
        self.relative_n(Direction::West, steps)
    }

    pub fn east(&self) -> BlockPos {
        self.relative(Direction::East)
    }

    pub fn east_n(&self, steps: i32) -> BlockPos {
        self.relative_n(Direction::East, steps)
    }

    pub fn relative(&self, direction: Direction) -> BlockPos {
        BlockPos(self.0.relative(direction))
    }

    pub fn relative_n(&self, direction: Direction, steps: i32) -> BlockPos {
        BlockPos(self.0.relative_n(direction, steps))
    }

    pub fn relative_axis(&self, axis: Axis, steps: i32) -> BlockPos {
        BlockPos(self.0.relative_axis(axis, steps))
    }

    pub const fn rotate(&self, rotation: Rotation) -> BlockPos {
        match rotation {
            Rotation::Clockwise90 => BlockPos::new(self.0.z.wrapping_neg(), self.0.y, self.0.x),
            Rotation::Clockwise180 => {
                BlockPos::new(self.0.x.wrapping_neg(), self.0.y, self.0.z.wrapping_neg())
            }
            Rotation::Counterclockwise90 => {
                BlockPos::new(self.0.z, self.0.y, self.0.x.wrapping_neg())
            }
            Rotation::None => *self,
        }
    }

    pub const fn cross(&self, up_vector: &Vec3i) -> BlockPos {
        BlockPos(self.0.cross(up_vector))
    }

    pub const fn at_y(&self, y: i32) -> BlockPos {
        BlockPos::new(self.0.x, y, self.0.z)
    }

    pub const fn immutable(&self) -> BlockPos {
        *self
    }

    pub const fn mutable(&self) -> MutableBlockPos {
        MutableBlockPos(*self)
    }

    /// `(double)(x + 1.0E-5F)` is a float addition (the int is widened to `float`).
    pub fn clamp_location_within(&self, location: &Vec3) -> Vec3 {
        let (x, y, z) = (self.0.x, self.0.y, self.0.z);
        Vec3 {
            x: mth::clamp_f64(
                location.x,
                (x as f32 + 1.0E-5f32) as f64,
                x as f64 + 1.0 - 1.0E-5f32 as f64,
            ),
            y: mth::clamp_f64(
                location.y,
                (y as f32 + 1.0E-5f32) as f64,
                y as f64 + 1.0 - 1.0E-5f32 as f64,
            ),
            z: mth::clamp_f64(
                location.z,
                (z as f32 + 1.0E-5f32) as f64,
                z as f64 + 1.0 - 1.0E-5f32 as f64,
            ),
        }
    }

    // ----- iterators (vanilla order; each step yields a fresh copy) -----

    /// `randomInCube`
    pub fn random_in_cube<'a>(
        random: &'a mut dyn RandomSource,
        limit: i32,
        center: &BlockPos,
        size_to_scan_in_all_directions: i32,
    ) -> impl Iterator<Item = BlockPos> + 'a {
        let d = size_to_scan_in_all_directions;
        Self::random_between_closed(
            random,
            limit,
            center.x().wrapping_sub(d),
            center.y().wrapping_sub(d),
            center.z().wrapping_sub(d),
            center.x().wrapping_add(d),
            center.y().wrapping_add(d),
            center.z().wrapping_add(d),
        )
    }

    /// `randomBetweenClosed`: draws x, then y, then z, each via `nextInt(extent)`.
    #[allow(clippy::too_many_arguments)]
    pub fn random_between_closed<'a>(
        random: &'a mut dyn RandomSource,
        limit: i32,
        min_x: i32,
        min_y: i32,
        min_z: i32,
        max_x: i32,
        max_y: i32,
        max_z: i32,
    ) -> impl Iterator<Item = BlockPos> + 'a {
        let width = max_x.wrapping_sub(min_x).wrapping_add(1);
        let height = max_y.wrapping_sub(min_y).wrapping_add(1);
        let depth = max_z.wrapping_sub(min_z).wrapping_add(1);
        let mut counter = limit;
        iter_from_fn(move || {
            if counter <= 0 {
                return None;
            }
            let x = min_x.wrapping_add(random.next_int_bounded(width));
            let y = min_y.wrapping_add(random.next_int_bounded(height));
            let z = min_z.wrapping_add(random.next_int_bounded(depth));
            counter -= 1;
            Some(BlockPos::new(x, y, z))
        })
    }

    pub fn within_manhattan(
        origin: &BlockPos,
        reach: i32,
    ) -> impl Iterator<Item = BlockPos> + use<> {
        Self::manhattan_ordered(origin, reach, reach, reach, reach)
    }

    pub fn within_clipped_manhattan(
        origin: &BlockPos,
        reach_x: i32,
        reach_y: i32,
        reach_z: i32,
    ) -> impl Iterator<Item = BlockPos> + use<> {
        Self::manhattan_ordered(
            origin,
            reach_x,
            reach_y,
            reach_z,
            reach_x.max(reach_y).max(reach_z),
        )
    }

    pub fn within_box_by_manhattan_distance(
        origin: &BlockPos,
        reach_x: i32,
        reach_y: i32,
        reach_z: i32,
    ) -> impl Iterator<Item = BlockPos> + use<> {
        Self::manhattan_ordered(
            origin,
            reach_x,
            reach_y,
            reach_z,
            reach_x.wrapping_add(reach_y).wrapping_add(reach_z),
        )
    }

    fn manhattan_ordered(
        origin: &BlockPos,
        reach_x: i32,
        reach_y: i32,
        reach_z: i32,
        max_depth: i32,
    ) -> impl Iterator<Item = BlockPos> + use<> {
        let (origin_x, origin_y, origin_z) = (origin.x(), origin.y(), origin.z());
        let mut cursor = MutableBlockPos::default();
        let (mut current_depth, mut max_x, mut max_y, mut x, mut y) =
            (0i32, 0i32, 0i32, 0i32, 0i32);
        let mut z_mirror = false;
        iter_from_fn(move || {
            if z_mirror {
                z_mirror = false;
                cursor.set_z(origin_z.wrapping_sub(cursor.z().wrapping_sub(origin_z)));
                return Some(cursor.immutable());
            }
            let mut found = None;
            while found.is_none() {
                if y > max_y {
                    x = x.wrapping_add(1);
                    if x > max_x {
                        current_depth = current_depth.wrapping_add(1);
                        if current_depth > max_depth {
                            return None;
                        }
                        max_x = reach_x.min(current_depth);
                        x = max_x.wrapping_neg();
                    }
                    max_y = reach_y.min(current_depth.wrapping_sub(x.wrapping_abs()));
                    y = max_y.wrapping_neg();
                }
                let xx = x;
                let yy = y;
                let zz = current_depth
                    .wrapping_sub(xx.wrapping_abs())
                    .wrapping_sub(yy.wrapping_abs());
                if zz <= reach_z {
                    z_mirror = zz != 0;
                    found = Some(
                        cursor
                            .set(
                                origin_x.wrapping_add(xx),
                                origin_y.wrapping_add(yy),
                                origin_z.wrapping_add(zz),
                            )
                            .immutable(),
                    );
                }
                y = y.wrapping_add(1);
            }
            found
        })
    }

    /// `betweenClosed(AABB)`; also covers `betweenClosedStream(AABB)` (same floor semantics).
    pub fn between_closed_aabb(aabb: &AABB) -> impl Iterator<Item = BlockPos> + use<> {
        let start = Self::containing(aabb.min_x, aabb.min_y, aabb.min_z);
        let end = Self::containing(aabb.max_x, aabb.max_y, aabb.max_z);
        Self::between_closed(&start, &end)
    }

    /// `betweenClosed(BlockPos, BlockPos)` / `betweenClosedStream(BlockPos, BlockPos)`
    pub fn between_closed(a: &BlockPos, b: &BlockPos) -> impl Iterator<Item = BlockPos> + use<> {
        Self::between_closed_xyz(
            a.x().min(b.x()),
            a.y().min(b.y()),
            a.z().min(b.z()),
            a.x().max(b.x()),
            a.y().max(b.y()),
            a.z().max(b.z()),
        )
    }

    /// `betweenClosedStream(BoundingBox)`
    pub fn between_closed_box(
        bounding_box: &BoundingBox,
    ) -> impl Iterator<Item = BlockPos> + use<> {
        Self::between_closed_xyz(
            bounding_box.min_x().min(bounding_box.max_x()),
            bounding_box.min_y().min(bounding_box.max_y()),
            bounding_box.min_z().min(bounding_box.max_z()),
            bounding_box.min_x().max(bounding_box.max_x()),
            bounding_box.min_y().max(bounding_box.max_y()),
            bounding_box.min_z().max(bounding_box.max_z()),
        )
    }

    /// `betweenClosed(int x6)` / `betweenClosedStream(int x6)`: x fastest, then y, then z.
    pub fn between_closed_xyz(
        min_x: i32,
        min_y: i32,
        min_z: i32,
        max_x: i32,
        max_y: i32,
        max_z: i32,
    ) -> impl Iterator<Item = BlockPos> + use<> {
        let width = max_x.wrapping_sub(min_x).wrapping_add(1);
        let height = max_y.wrapping_sub(min_y).wrapping_add(1);
        let depth = max_z.wrapping_sub(min_z).wrapping_add(1);
        let end = width.wrapping_mul(height).wrapping_mul(depth);
        let mut index = 0i32;
        iter_from_fn(move || {
            if index == end {
                return None;
            }
            // end != 0 here, so width and height are non-zero
            let x = index.wrapping_rem(width);
            let slice = index.wrapping_div(width);
            let y = slice.wrapping_rem(height);
            let z = slice.wrapping_div(height);
            index = index.wrapping_add(1);
            Some(BlockPos::new(
                min_x.wrapping_add(x),
                min_y.wrapping_add(y),
                min_z.wrapping_add(z),
            ))
        })
    }

    pub fn neighbor_column(
        start_x: i32,
        start_y: i32,
        start_z: i32,
        end_y: i32,
    ) -> impl Iterator<Item = BlockPos> + use<> {
        let y_direction = if end_y > start_y { 1 } else { -1 };
        let height = end_y.wrapping_sub(start_y).wrapping_abs().wrapping_add(1);
        let steps = [
            Vec3i::ZERO,
            Direction::North.get_unit_vec3i(),
            Direction::East.get_unit_vec3i(),
            Direction::South.get_unit_vec3i(),
            Direction::West.get_unit_vec3i(),
        ];
        let step_count = (steps.len() as i32).wrapping_mul(height);
        let mut index = 0i32;
        iter_from_fn(move || {
            if index == step_count {
                return None;
            }
            let y = index.wrapping_rem(height);
            let step_index = index.wrapping_div(height);
            let step = steps[step_index as usize];
            index = index.wrapping_add(1);
            Some(BlockPos::new(
                start_x.wrapping_add(step.x),
                start_y.wrapping_add(y.wrapping_mul(y_direction)),
                start_z.wrapping_add(step.z),
            ))
        })
    }

    /// `spiralAround`. Panics (vanilla: `IllegalStateException`) if both directions share an axis.
    pub fn spiral_around(
        center: &BlockPos,
        radius: i32,
        first_direction: Direction,
        second_direction: Direction,
    ) -> impl Iterator<Item = BlockPos> + use<> {
        assert!(
            first_direction.get_axis() != second_direction.get_axis(),
            "The two directions cannot be on the same axis"
        );
        let directions = [
            first_direction,
            second_direction,
            first_direction.get_opposite(),
            second_direction.get_opposite(),
        ];
        let mut cursor = center.mutable();
        cursor.move_dir(second_direction);
        let legs = radius.wrapping_mul(4);
        let (mut leg, mut leg_size, mut leg_index) = (-1i32, 0i32, 0i32);
        iter_from_fn(move || {
            cursor.move_dir(directions[((leg + 4) % 4) as usize]);
            if leg_index >= leg_size {
                if leg >= legs {
                    return None;
                }
                leg += 1;
                leg_index = 0;
                leg_size = leg / 2 + 1;
            }
            leg_index += 1;
            Some(cursor.immutable())
        })
    }

    /// `breadthFirstTraversal`. `neighbour_provider` receives a sink for the neighbours to enqueue.
    pub fn breadth_first_traversal(
        start_pos: BlockPos,
        max_depth: i32,
        max_count: i32,
        mut neighbour_provider: impl FnMut(BlockPos, &mut dyn FnMut(BlockPos)),
        mut node_processor: impl FnMut(BlockPos) -> TraversalNodeStatus,
    ) -> i32 {
        let mut nodes: VecDeque<(BlockPos, i32)> = VecDeque::new();
        let mut visited: HashSet<i64> = HashSet::new();
        nodes.push_back((start_pos, 0));
        let mut count = 0i32;
        while let Some((current_pos, depth)) = nodes.pop_front() {
            if visited.insert(current_pos.as_long()) {
                let next = node_processor(current_pos);
                if next != TraversalNodeStatus::Skip {
                    if next == TraversalNodeStatus::Stop {
                        break;
                    }
                    count = count.wrapping_add(1);
                    if count >= max_count {
                        return count;
                    }
                    if depth < max_depth {
                        neighbour_provider(current_pos, &mut |pos| {
                            nodes.push_back((pos, depth + 1))
                        });
                    }
                }
            }
        }
        count
    }

    /// `betweenCornersInDirection(AABB, Vec3)`
    pub fn between_corners_in_direction_aabb(
        aabb: &AABB,
        direction: Vec3,
    ) -> impl Iterator<Item = BlockPos> + use<> {
        let min = aabb.get_min_position();
        let max = aabb.get_max_position();
        Self::between_corners_in_direction_xyz(
            mth::floor_f64(min.x),
            mth::floor_f64(min.y),
            mth::floor_f64(min.z),
            mth::floor_f64(max.x),
            mth::floor_f64(max.y),
            mth::floor_f64(max.z),
            direction,
        )
    }

    pub fn between_corners_in_direction(
        first_corner: &BlockPos,
        second_corner: &BlockPos,
        direction: Vec3,
    ) -> impl Iterator<Item = BlockPos> + use<> {
        Self::between_corners_in_direction_xyz(
            first_corner.x(),
            first_corner.y(),
            first_corner.z(),
            second_corner.x(),
            second_corner.y(),
            second_corner.z(),
            direction,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn between_corners_in_direction_xyz(
        first_corner_x: i32,
        first_corner_y: i32,
        first_corner_z: i32,
        second_corner_x: i32,
        second_corner_y: i32,
        second_corner_z: i32,
        direction: Vec3,
    ) -> impl Iterator<Item = BlockPos> + use<> {
        let min_x = first_corner_x.min(second_corner_x);
        let min_y = first_corner_y.min(second_corner_y);
        let min_z = first_corner_z.min(second_corner_z);
        let max_x = first_corner_x.max(second_corner_x);
        let max_y = first_corner_y.max(second_corner_y);
        let max_z = first_corner_z.max(second_corner_z);
        let (diff_x, diff_y, diff_z) = (
            max_x.wrapping_sub(min_x),
            max_y.wrapping_sub(min_y),
            max_z.wrapping_sub(min_z),
        );
        let start_x = if direction.x >= 0.0 { min_x } else { max_x };
        let start_y = if direction.y >= 0.0 { min_y } else { max_y };
        let start_z = if direction.z >= 0.0 { min_z } else { max_z };
        let axes = Direction::axis_step_order(direction);
        let visit_dir = |axis: Axis| {
            if direction.get(axis) >= 0.0 {
                axis.get_positive()
            } else {
                axis.get_negative()
            }
        };
        let (first_dir, second_dir, third_dir) =
            (visit_dir(axes[0]), visit_dir(axes[1]), visit_dir(axes[2]));
        let first_max = axes[0].choose(diff_x, diff_y, diff_z);
        let second_max = axes[1].choose(diff_x, diff_y, diff_z);
        let third_max = axes[2].choose(diff_x, diff_y, diff_z);
        let (mut first_index, mut second_index, mut third_index) = (0i32, 0i32, 0i32);
        let mut end = false;
        iter_from_fn(move || {
            if end {
                return None;
            }
            let along = |step: fn(Direction) -> i32| {
                step(first_dir)
                    .wrapping_mul(first_index)
                    .wrapping_add(step(second_dir).wrapping_mul(second_index))
                    .wrapping_add(step(third_dir).wrapping_mul(third_index))
            };
            let pos = BlockPos::new(
                start_x.wrapping_add(along(Direction::get_step_x)),
                start_y.wrapping_add(along(Direction::get_step_y)),
                start_z.wrapping_add(along(Direction::get_step_z)),
            );
            if third_index < third_max {
                third_index += 1;
            } else if second_index < second_max {
                second_index += 1;
                third_index = 0;
            } else if first_index < first_max {
                first_index += 1;
                third_index = 0;
                second_index = 0;
            } else {
                end = true;
            }
            Some(pos)
        })
    }
}

/// `BlockPos.MutableBlockPos`. No `Ord` on purpose: `Ord::clamp(self, ..)` would shadow the vanilla `clamp(Axis, ..)`.
#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct MutableBlockPos(BlockPos);

impl Deref for MutableBlockPos {
    type Target = BlockPos;

    fn deref(&self) -> &BlockPos {
        &self.0
    }
}

impl fmt::Display for MutableBlockPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_named(f, "MutableBlockPos", self.0.0.x, self.0.0.y, self.0.0.z)
    }
}

impl fmt::Debug for MutableBlockPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl MutableBlockPos {
    pub const fn new(x: i32, y: i32, z: i32) -> MutableBlockPos {
        MutableBlockPos(BlockPos::new(x, y, z))
    }

    /// `new MutableBlockPos(double, double, double)` (floors each component)
    pub fn from_f64(x: f64, y: f64, z: f64) -> MutableBlockPos {
        MutableBlockPos(BlockPos::containing(x, y, z))
    }

    pub const fn immutable(&self) -> BlockPos {
        self.0
    }

    pub const fn set(&mut self, x: i32, y: i32, z: i32) -> &mut Self {
        self.0 = BlockPos::new(x, y, z);
        self
    }

    pub fn set_f64(&mut self, x: f64, y: f64, z: f64) -> &mut Self {
        self.set(mth::floor_f64(x), mth::floor_f64(y), mth::floor_f64(z))
    }

    pub const fn set_vec(&mut self, vec: &Vec3i) -> &mut Self {
        self.set(vec.x, vec.y, vec.z)
    }

    pub const fn set_long(&mut self, pos: i64) -> &mut Self {
        self.set(
            BlockPos::get_x(pos),
            BlockPos::get_y(pos),
            BlockPos::get_z(pos),
        )
    }

    pub fn set_axis_cycle(&mut self, transform: AxisCycle, x: i32, y: i32, z: i32) -> &mut Self {
        self.set(
            transform.cycle_i32(x, y, z, Axis::X),
            transform.cycle_i32(x, y, z, Axis::Y),
            transform.cycle_i32(x, y, z, Axis::Z),
        )
    }

    pub fn set_with_offset_direction(&mut self, pos: &Vec3i, direction: Direction) -> &mut Self {
        self.set(
            pos.x.wrapping_add(direction.get_step_x()),
            pos.y.wrapping_add(direction.get_step_y()),
            pos.z.wrapping_add(direction.get_step_z()),
        )
    }

    pub const fn set_with_offset(&mut self, pos: &Vec3i, x: i32, y: i32, z: i32) -> &mut Self {
        self.set(
            pos.x.wrapping_add(x),
            pos.y.wrapping_add(y),
            pos.z.wrapping_add(z),
        )
    }

    pub const fn set_with_offset_vec(&mut self, pos: &Vec3i, offset: &Vec3i) -> &mut Self {
        self.set(
            pos.x.wrapping_add(offset.x),
            pos.y.wrapping_add(offset.y),
            pos.z.wrapping_add(offset.z),
        )
    }

    /// `move(Direction)`
    pub fn move_dir(&mut self, direction: Direction) -> &mut Self {
        self.move_dir_n(direction, 1)
    }

    /// `move(Direction, int)`
    pub fn move_dir_n(&mut self, direction: Direction, steps: i32) -> &mut Self {
        self.set(
            self.x()
                .wrapping_add(direction.get_step_x().wrapping_mul(steps)),
            self.y()
                .wrapping_add(direction.get_step_y().wrapping_mul(steps)),
            self.z()
                .wrapping_add(direction.get_step_z().wrapping_mul(steps)),
        )
    }

    /// `move(int, int, int)`
    pub const fn move_by(&mut self, x: i32, y: i32, z: i32) -> &mut Self {
        self.set(
            self.0.0.x.wrapping_add(x),
            self.0.0.y.wrapping_add(y),
            self.0.0.z.wrapping_add(z),
        )
    }

    /// `move(Vec3i)`
    pub const fn move_vec(&mut self, pos: &Vec3i) -> &mut Self {
        self.move_by(pos.x, pos.y, pos.z)
    }

    pub fn clamp(&mut self, axis: Axis, minimum: i32, maximum: i32) -> &mut Self {
        let (x, y, z) = (self.x(), self.y(), self.z());
        match axis {
            Axis::X => self.set(mth::clamp_i32(x, minimum, maximum), y, z),
            Axis::Y => self.set(x, mth::clamp_i32(y, minimum, maximum), z),
            Axis::Z => self.set(x, y, mth::clamp_i32(z, minimum, maximum)),
        }
    }

    pub const fn set_x(&mut self, x: i32) -> &mut Self {
        self.set(x, self.0.0.y, self.0.0.z)
    }

    pub const fn set_y(&mut self, y: i32) -> &mut Self {
        self.set(self.0.0.x, y, self.0.0.z)
    }

    pub const fn set_z(&mut self, z: i32) -> &mut Self {
        self.set(self.0.0.x, self.0.0.y, z)
    }
}
