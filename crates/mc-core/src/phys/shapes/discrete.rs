//! `DiscreteVoxelShape` (+ `BitSetDiscreteVoxelShape`, `SubShape`) and the `java.util.BitSet` subset they use.

use std::sync::Arc;

use super::{BooleanOp, IndexMerger};
use crate::direction::{Axis, AxisCycle, AxisDirection, Direction};
use crate::math::octahedral_group::OctahedralGroup;

/// The subset of `java.util.BitSet` used by `BitSetDiscreteVoxelShape` (little-endian 64-bit words).
#[derive(Clone, Default, Debug)]
pub struct BitSet {
    words: Vec<u64>,
}

impl BitSet {
    pub fn with_capacity(nbits: usize) -> BitSet {
        BitSet {
            words: Vec::with_capacity(nbits.div_ceil(64)),
        }
    }

    /// Builds from `BitSet.valueOf(long[])`-style words.
    pub fn from_words(words: &[u64]) -> BitSet {
        BitSet {
            words: words.to_vec(),
        }
    }

    /// `BitSet.toLongArray()`: words without trailing zero words.
    pub fn words(&self) -> &[u64] {
        let mut n = self.words.len();
        while n > 0 && self.words[n - 1] == 0 {
            n -= 1;
        }
        &self.words[..n]
    }

    #[inline]
    pub fn get(&self, index: usize) -> bool {
        self.words
            .get(index >> 6)
            .is_some_and(|w| w >> (index & 63) & 1 != 0)
    }

    pub fn set(&mut self, index: usize) {
        let w = index >> 6;
        if w >= self.words.len() {
            self.words.resize(w + 1, 0);
        }
        self.words[w] |= 1u64 << (index & 63);
    }

    /// `BitSet.clear(from, to)` (`to` exclusive).
    pub fn clear_range(&mut self, from: usize, to: usize) {
        let to = to.min(self.words.len() * 64);
        if from >= to {
            return;
        }
        let (fw, tw) = (from >> 6, (to - 1) >> 6);
        let first_mask = !0u64 << (from & 63);
        let last_mask = !0u64 >> (63 - ((to - 1) & 63));
        if fw == tw {
            self.words[fw] &= !(first_mask & last_mask);
        } else {
            self.words[fw] &= !first_mask;
            for w in &mut self.words[fw + 1..tw] {
                *w = 0;
            }
            self.words[tw] &= !last_mask;
        }
    }

    pub fn next_clear_bit(&self, from: usize) -> usize {
        let mut w = from >> 6;
        if w >= self.words.len() {
            return from;
        }
        let mut word = !self.words[w] & (!0u64 << (from & 63));
        loop {
            if word != 0 {
                return w * 64 + word.trailing_zeros() as usize;
            }
            w += 1;
            if w == self.words.len() {
                return w * 64;
            }
            word = !self.words[w];
        }
    }

    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|&w| w == 0)
    }
}

impl PartialEq for BitSet {
    fn eq(&self, other: &BitSet) -> bool {
        self.words() == other.words()
    }
}
impl Eq for BitSet {}

fn check_sizes(x: i32, y: i32, z: i32) {
    assert!(
        x >= 0 && y >= 0 && z >= 0,
        "Need all positive sizes: x: {x}, y: {y}, z: {z}"
    );
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BitSetDiscreteVoxelShape {
    x_size: i32,
    y_size: i32,
    z_size: i32,
    storage: BitSet,
    x_min: i32,
    y_min: i32,
    z_min: i32,
    x_max: i32,
    y_max: i32,
    z_max: i32,
}

impl BitSetDiscreteVoxelShape {
    pub fn new(x_size: i32, y_size: i32, z_size: i32) -> Self {
        check_sizes(x_size, y_size, z_size);
        BitSetDiscreteVoxelShape {
            x_size,
            y_size,
            z_size,
            storage: BitSet::with_capacity(
                (x_size as usize) * (y_size as usize) * (z_size as usize),
            ),
            x_min: x_size,
            y_min: y_size,
            z_min: z_size,
            x_max: 0,
            y_max: 0,
            z_max: 0,
        }
    }

    /// Exact internal state: sizes, bit words (`BitSet.toLongArray()` layout) and the six bounds.
    #[allow(clippy::too_many_arguments)]
    pub fn from_raw(
        x_size: i32,
        y_size: i32,
        z_size: i32,
        bits: &[u64],
        x_min: i32,
        y_min: i32,
        z_min: i32,
        x_max: i32,
        y_max: i32,
        z_max: i32,
    ) -> Self {
        check_sizes(x_size, y_size, z_size);
        BitSetDiscreteVoxelShape {
            x_size,
            y_size,
            z_size,
            storage: BitSet::from_words(bits),
            x_min,
            y_min,
            z_min,
            x_max,
            y_max,
            z_max,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_filled_bounds(
        x_size: i32,
        y_size: i32,
        z_size: i32,
        x_min: i32,
        y_min: i32,
        z_min: i32,
        x_max: i32,
        y_max: i32,
        z_max: i32,
    ) -> Self {
        let mut shape = Self::new(x_size, y_size, z_size);
        shape.x_min = x_min;
        shape.y_min = y_min;
        shape.z_min = z_min;
        shape.x_max = x_max;
        shape.y_max = y_max;
        shape.z_max = z_max;
        for x in x_min..x_max {
            for y in y_min..y_max {
                for z in z_min..z_max {
                    shape.fill_update_bounds(x, y, z, false);
                }
            }
        }
        shape
    }

    /// `new BitSetDiscreteVoxelShape(DiscreteVoxelShape)`.
    pub fn from_discrete(voxel_shape: &DiscreteVoxelShape) -> Self {
        let (xs, ys, zs) = (
            voxel_shape.get_x_size(),
            voxel_shape.get_y_size(),
            voxel_shape.get_z_size(),
        );
        let storage = if let DiscreteVoxelShape::BitSet(b) = voxel_shape {
            b.storage.clone()
        } else {
            let mut s = BitSet::with_capacity((xs as usize) * (ys as usize) * (zs as usize));
            for x in 0..xs {
                for y in 0..ys {
                    for z in 0..zs {
                        if voxel_shape.is_full(x, y, z) {
                            s.set(Self::index(ys, zs, x, y, z));
                        }
                    }
                }
            }
            s
        };
        BitSetDiscreteVoxelShape {
            x_size: xs,
            y_size: ys,
            z_size: zs,
            storage,
            x_min: voxel_shape.first_full(Axis::X),
            y_min: voxel_shape.first_full(Axis::Y),
            z_min: voxel_shape.first_full(Axis::Z),
            x_max: voxel_shape.last_full(Axis::X),
            y_max: voxel_shape.last_full(Axis::Y),
            z_max: voxel_shape.last_full(Axis::Z),
        }
    }

    #[inline]
    fn index(y_size: i32, z_size: i32, x: i32, y: i32, z: i32) -> usize {
        x.wrapping_mul(y_size)
            .wrapping_add(y)
            .wrapping_mul(z_size)
            .wrapping_add(z) as usize
    }

    #[inline]
    fn get_index(&self, x: i32, y: i32, z: i32) -> usize {
        Self::index(self.y_size, self.z_size, x, y, z)
    }

    #[inline]
    pub fn is_full(&self, x: i32, y: i32, z: i32) -> bool {
        self.storage.get(self.get_index(x, y, z))
    }

    fn fill_update_bounds(&mut self, x: i32, y: i32, z: i32, update_bounds: bool) {
        self.storage.set(self.get_index(x, y, z));
        if update_bounds {
            self.x_min = self.x_min.min(x);
            self.y_min = self.y_min.min(y);
            self.z_min = self.z_min.min(z);
            self.x_max = self.x_max.max(x + 1);
            self.y_max = self.y_max.max(y + 1);
            self.z_max = self.z_max.max(z + 1);
        }
    }

    pub fn fill(&mut self, x: i32, y: i32, z: i32) {
        self.fill_update_bounds(x, y, z, true);
    }

    pub fn is_empty(&self) -> bool {
        self.storage.is_empty()
    }

    pub fn x_size(&self) -> i32 {
        self.x_size
    }
    pub fn y_size(&self) -> i32 {
        self.y_size
    }
    pub fn z_size(&self) -> i32 {
        self.z_size
    }
    /// Bit words, `BitSet.toLongArray()` layout; bit index is `(x * ySize + y) * zSize + z`.
    pub fn bits(&self) -> &[u64] {
        self.storage.words()
    }
    pub fn x_min(&self) -> i32 {
        self.x_min
    }
    pub fn y_min(&self) -> i32 {
        self.y_min
    }
    pub fn z_min(&self) -> i32 {
        self.z_min
    }
    pub fn x_max(&self) -> i32 {
        self.x_max
    }
    pub fn y_max(&self) -> i32 {
        self.y_max
    }
    pub fn z_max(&self) -> i32 {
        self.z_max
    }

    pub fn first_full(&self, axis: Axis) -> i32 {
        axis.choose(self.x_min, self.y_min, self.z_min)
    }

    pub fn last_full(&self, axis: Axis) -> i32 {
        axis.choose(self.x_max, self.y_max, self.z_max)
    }

    pub fn join(
        first: &DiscreteVoxelShape,
        second: &DiscreteVoxelShape,
        x_merger: &IndexMerger,
        y_merger: &IndexMerger,
        z_merger: &IndexMerger,
        op: BooleanOp,
    ) -> BitSetDiscreteVoxelShape {
        let mut shape = BitSetDiscreteVoxelShape::new(
            x_merger.size() as i32 - 1,
            y_merger.size() as i32 - 1,
            z_merger.size() as i32 - 1,
        );
        let mut bounds = [i32::MAX, i32::MAX, i32::MAX, i32::MIN, i32::MIN, i32::MIN];
        x_merger.for_merged_indexes(&mut |x1, x2, xr| {
            let mut updated_slice = false;
            y_merger.for_merged_indexes(&mut |y1, y2, yr| {
                let mut updated_column = false;
                z_merger.for_merged_indexes(&mut |z1, z2, zr| {
                    if op.apply(
                        first.is_full_wide(x1, y1, z1),
                        second.is_full_wide(x2, y2, z2),
                    ) {
                        let i = shape.get_index(xr, yr, zr);
                        shape.storage.set(i);
                        bounds[2] = bounds[2].min(zr);
                        bounds[5] = bounds[5].max(zr);
                        updated_column = true;
                    }
                    true
                });
                if updated_column {
                    bounds[1] = bounds[1].min(yr);
                    bounds[4] = bounds[4].max(yr);
                    updated_slice = true;
                }
                true
            });
            if updated_slice {
                bounds[0] = bounds[0].min(xr);
                bounds[3] = bounds[3].max(xr);
            }
            true
        });
        shape.x_min = bounds[0];
        shape.y_min = bounds[1];
        shape.z_min = bounds[2];
        shape.x_max = bounds[3].wrapping_add(1);
        shape.y_max = bounds[4].wrapping_add(1);
        shape.z_max = bounds[5].wrapping_add(1);
        shape
    }

    fn for_all_boxes(
        voxel_shape: &DiscreteVoxelShape,
        consumer: &mut impl FnMut(i32, i32, i32, i32, i32, i32),
        merge_neighbors: bool,
    ) {
        let mut shape = BitSetDiscreteVoxelShape::from_discrete(voxel_shape);
        for y in 0..shape.y_size {
            for x in 0..shape.x_size {
                let mut last_start_z = -1;
                for z in 0..=shape.z_size {
                    if shape.is_full_wide(x, y, z) {
                        if merge_neighbors {
                            if last_start_z == -1 {
                                last_start_z = z;
                            }
                        } else {
                            consumer(x, y, z, x + 1, y + 1, z + 1);
                        }
                    } else if last_start_z != -1 {
                        let mut end_x = x;
                        let mut end_y = y;
                        shape.clear_z_strip(last_start_z, z, x, y);
                        while shape.is_z_strip_full(last_start_z, z, end_x + 1, y) {
                            shape.clear_z_strip(last_start_z, z, end_x + 1, y);
                            end_x += 1;
                        }
                        while shape.is_xz_rectangle_full(x, end_x + 1, last_start_z, z, end_y + 1) {
                            for cx in x..=end_x {
                                shape.clear_z_strip(last_start_z, z, cx, end_y + 1);
                            }
                            end_y += 1;
                        }
                        consumer(x, y, last_start_z, end_x + 1, end_y + 1, z);
                        last_start_z = -1;
                    }
                }
            }
        }
    }

    fn is_full_wide(&self, x: i32, y: i32, z: i32) -> bool {
        if x < 0 || y < 0 || z < 0 {
            return false;
        }
        x < self.x_size && y < self.y_size && z < self.z_size && self.is_full(x, y, z)
    }

    fn is_z_strip_full(&self, start_z: i32, end_z: i32, x: i32, y: i32) -> bool {
        x < self.x_size
            && y < self.y_size
            && self.storage.next_clear_bit(self.get_index(x, y, start_z))
                >= self.get_index(x, y, end_z)
    }

    fn is_xz_rectangle_full(
        &self,
        start_x: i32,
        end_x: i32,
        start_z: i32,
        end_z: i32,
        y: i32,
    ) -> bool {
        (start_x..end_x).all(|x| self.is_z_strip_full(start_z, end_z, x, y))
    }

    fn clear_z_strip(&mut self, start_z: i32, end_z: i32, x: i32, y: i32) {
        let (a, b) = (self.get_index(x, y, start_z), self.get_index(x, y, end_z));
        self.storage.clear_range(a, b);
    }

    pub fn is_interior(&self, x: i32, y: i32, z: i32) -> bool {
        let is_interior = x > 0
            && x < self.x_size - 1
            && y > 0
            && y < self.y_size - 1
            && z > 0
            && z < self.z_size - 1;
        is_interior
            && self.is_full(x, y, z)
            && self.is_full(x - 1, y, z)
            && self.is_full(x + 1, y, z)
            && self.is_full(x, y - 1, z)
            && self.is_full(x, y + 1, z)
            && self.is_full(x, y, z - 1)
            && self.is_full(x, y, z + 1)
    }
}

/// A read-only window `[start, end)` onto a parent shape. (Vanilla's `fill` writes through to the parent;
/// nothing in vanilla ever fills a `SubShape`, so shapes here are immutable once shared.)
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SubShape {
    parent: Arc<DiscreteVoxelShape>,
    start: [i32; 3],
    end: [i32; 3],
}

impl SubShape {
    pub fn new(
        parent: Arc<DiscreteVoxelShape>,
        start_x: i32,
        start_y: i32,
        start_z: i32,
        end_x: i32,
        end_y: i32,
        end_z: i32,
    ) -> Self {
        check_sizes(end_x - start_x, end_y - start_y, end_z - start_z);
        SubShape {
            parent,
            start: [start_x, start_y, start_z],
            end: [end_x, end_y, end_z],
        }
    }

    fn clamp_to_shape(&self, axis: Axis, parent_result: i32) -> i32 {
        let i = axis as usize;
        let (start, end) = (self.start[i], self.end[i]);
        // Mth.clamp(int, min, max)
        (if parent_result < start {
            start
        } else {
            parent_result.min(end)
        }) - start
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DiscreteVoxelShape {
    BitSet(BitSetDiscreteVoxelShape),
    Sub(SubShape),
}

impl From<BitSetDiscreteVoxelShape> for DiscreteVoxelShape {
    fn from(s: BitSetDiscreteVoxelShape) -> Self {
        DiscreteVoxelShape::BitSet(s)
    }
}

impl DiscreteVoxelShape {
    /// `rotate(OctahedralGroup)`. Vanilla returns `this` for `IDENTITY`; here that is a clone (callers that need
    /// identity, like `Shapes::rotate`, short-circuit first).
    pub fn rotate(&self, rotation: OctahedralGroup) -> DiscreteVoxelShape {
        if rotation == OctahedralGroup::Identity {
            return self.clone();
        }
        let mut v =
            rotation.rotate_vec3i([self.get_x_size(), self.get_y_size(), self.get_z_size()]);
        let shift = [0, 1, 2].map(|i| Self::fixup_coordinate(&mut v, i));
        let mut new_shape = BitSetDiscreteVoxelShape::new(v[0], v[1], v[2]);
        for x in 0..self.get_x_size() {
            for y in 0..self.get_y_size() {
                for z in 0..self.get_z_size() {
                    if self.is_full(x, y, z) {
                        let new_pos = rotation.rotate_vec3i([x, y, z]);
                        new_shape.fill(
                            shift[0] + new_pos[0],
                            shift[1] + new_pos[1],
                            shift[2] + new_pos[2],
                        );
                    }
                }
            }
        }
        DiscreteVoxelShape::BitSet(new_shape)
    }

    fn fixup_coordinate(v: &mut [i32; 3], index: usize) -> i32 {
        let value = v[index];
        if value < 0 {
            v[index] = -value;
            -value - 1
        } else {
            0
        }
    }

    pub fn is_full_wide_cycled(&self, transform: AxisCycle, x: i32, y: i32, z: i32) -> bool {
        self.is_full_wide(
            transform.cycle_i32(x, y, z, Axis::X),
            transform.cycle_i32(x, y, z, Axis::Y),
            transform.cycle_i32(x, y, z, Axis::Z),
        )
    }

    pub fn is_full_wide(&self, x: i32, y: i32, z: i32) -> bool {
        if x < 0 || y < 0 || z < 0 {
            return false;
        }
        x < self.get_x_size()
            && y < self.get_y_size()
            && z < self.get_z_size()
            && self.is_full(x, y, z)
    }

    pub fn is_full_cycled(&self, transform: AxisCycle, x: i32, y: i32, z: i32) -> bool {
        self.is_full(
            transform.cycle_i32(x, y, z, Axis::X),
            transform.cycle_i32(x, y, z, Axis::Y),
            transform.cycle_i32(x, y, z, Axis::Z),
        )
    }

    pub fn is_full(&self, x: i32, y: i32, z: i32) -> bool {
        match self {
            DiscreteVoxelShape::BitSet(b) => b.is_full(x, y, z),
            DiscreteVoxelShape::Sub(s) => {
                s.parent
                    .is_full(s.start[0] + x, s.start[1] + y, s.start[2] + z)
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            DiscreteVoxelShape::BitSet(b) => b.is_empty(),
            DiscreteVoxelShape::Sub(_) => Axis::VALUES
                .iter()
                .any(|&axis| self.first_full(axis) >= self.last_full(axis)),
        }
    }

    pub fn first_full(&self, axis: Axis) -> i32 {
        match self {
            DiscreteVoxelShape::BitSet(b) => b.first_full(axis),
            DiscreteVoxelShape::Sub(s) => s.clamp_to_shape(axis, s.parent.first_full(axis)),
        }
    }

    pub fn last_full(&self, axis: Axis) -> i32 {
        match self {
            DiscreteVoxelShape::BitSet(b) => b.last_full(axis),
            DiscreteVoxelShape::Sub(s) => s.clamp_to_shape(axis, s.parent.last_full(axis)),
        }
    }

    pub fn first_full_bc(&self, a_axis: Axis, b: i32, c: i32) -> i32 {
        let a_size = self.get_size(a_axis);
        if b >= 0 && c >= 0 {
            let b_axis = AxisCycle::Forward.cycle_axis(a_axis);
            let c_axis = AxisCycle::Backward.cycle_axis(a_axis);
            if b < self.get_size(b_axis) && c < self.get_size(c_axis) {
                let transform = AxisCycle::between(Axis::X, a_axis);
                for a in 0..a_size {
                    if self.is_full_cycled(transform, a, b, c) {
                        return a;
                    }
                }
            }
        }
        a_size
    }

    pub fn last_full_bc(&self, a_axis: Axis, b: i32, c: i32) -> i32 {
        if b >= 0 && c >= 0 {
            let b_axis = AxisCycle::Forward.cycle_axis(a_axis);
            let c_axis = AxisCycle::Backward.cycle_axis(a_axis);
            if b < self.get_size(b_axis) && c < self.get_size(c_axis) {
                let a_size = self.get_size(a_axis);
                let transform = AxisCycle::between(Axis::X, a_axis);
                for a in (0..a_size).rev() {
                    if self.is_full_cycled(transform, a, b, c) {
                        return a + 1;
                    }
                }
            }
        }
        0
    }

    pub fn get_size(&self, axis: Axis) -> i32 {
        axis.choose(self.get_x_size(), self.get_y_size(), self.get_z_size())
    }

    pub fn get_x_size(&self) -> i32 {
        match self {
            DiscreteVoxelShape::BitSet(b) => b.x_size,
            DiscreteVoxelShape::Sub(s) => s.end[0] - s.start[0],
        }
    }

    pub fn get_y_size(&self) -> i32 {
        match self {
            DiscreteVoxelShape::BitSet(b) => b.y_size,
            DiscreteVoxelShape::Sub(s) => s.end[1] - s.start[1],
        }
    }

    pub fn get_z_size(&self) -> i32 {
        match self {
            DiscreteVoxelShape::BitSet(b) => b.z_size,
            DiscreteVoxelShape::Sub(s) => s.end[2] - s.start[2],
        }
    }

    pub fn for_all_edges(
        &self,
        consumer: &mut impl FnMut(i32, i32, i32, i32, i32, i32),
        merge_neighbors: bool,
    ) {
        self.for_all_axis_edges(consumer, AxisCycle::None, merge_neighbors);
        self.for_all_axis_edges(consumer, AxisCycle::Forward, merge_neighbors);
        self.for_all_axis_edges(consumer, AxisCycle::Backward, merge_neighbors);
    }

    fn for_all_axis_edges(
        &self,
        consumer: &mut impl FnMut(i32, i32, i32, i32, i32, i32),
        transform: AxisCycle,
        merge_neighbors: bool,
    ) {
        let inverse = transform.inverse();
        let a_size = self.get_size(inverse.cycle_axis(Axis::X));
        let b_size = self.get_size(inverse.cycle_axis(Axis::Y));
        let c_size = self.get_size(inverse.cycle_axis(Axis::Z));
        let cyc = |a, b, c| {
            (
                inverse.cycle_i32(a, b, c, Axis::X),
                inverse.cycle_i32(a, b, c, Axis::Y),
                inverse.cycle_i32(a, b, c, Axis::Z),
            )
        };

        for a in 0..=a_size {
            for b in 0..=b_size {
                let mut last_start = -1;
                for c in 0..=c_size {
                    let mut full_sectors = 0;
                    let mut odd_sectors = 0;
                    for da in 0..=1 {
                        for db in 0..=1 {
                            if self.is_full_wide_cycled(inverse, a + da - 1, b + db - 1, c) {
                                full_sectors += 1;
                                odd_sectors ^= da ^ db;
                            }
                        }
                    }

                    if full_sectors == 1
                        || full_sectors == 3
                        || full_sectors == 2 && (odd_sectors & 1) == 0
                    {
                        if merge_neighbors {
                            if last_start == -1 {
                                last_start = c;
                            }
                        } else {
                            let (x1, y1, z1) = cyc(a, b, c);
                            let (x2, y2, z2) = cyc(a, b, c + 1);
                            consumer(x1, y1, z1, x2, y2, z2);
                        }
                    } else if last_start != -1 {
                        let (x1, y1, z1) = cyc(a, b, last_start);
                        let (x2, y2, z2) = cyc(a, b, c);
                        consumer(x1, y1, z1, x2, y2, z2);
                        last_start = -1;
                    }
                }
            }
        }
    }

    pub fn for_all_boxes(
        &self,
        consumer: &mut impl FnMut(i32, i32, i32, i32, i32, i32),
        merge_neighbors: bool,
    ) {
        BitSetDiscreteVoxelShape::for_all_boxes(self, consumer, merge_neighbors);
    }

    pub fn for_all_faces(&self, consumer: &mut impl FnMut(Direction, i32, i32, i32)) {
        self.for_all_axis_faces(consumer, AxisCycle::None);
        self.for_all_axis_faces(consumer, AxisCycle::Forward);
        self.for_all_axis_faces(consumer, AxisCycle::Backward);
    }

    fn for_all_axis_faces(
        &self,
        consumer: &mut impl FnMut(Direction, i32, i32, i32),
        transform: AxisCycle,
    ) {
        let inverse = transform.inverse();
        let c_axis = inverse.cycle_axis(Axis::Z);
        let a_size = self.get_size(inverse.cycle_axis(Axis::X));
        let b_size = self.get_size(inverse.cycle_axis(Axis::Y));
        let c_size = self.get_size(c_axis);
        let negative = Direction::from_axis_and_direction(c_axis, AxisDirection::Negative);
        let positive = Direction::from_axis_and_direction(c_axis, AxisDirection::Positive);
        let cyc = |a, b, c| {
            (
                inverse.cycle_i32(a, b, c, Axis::X),
                inverse.cycle_i32(a, b, c, Axis::Y),
                inverse.cycle_i32(a, b, c, Axis::Z),
            )
        };

        for a in 0..a_size {
            for b in 0..b_size {
                let mut last_full = false;
                for c in 0..=c_size {
                    let full = c != c_size && self.is_full_cycled(inverse, a, b, c);
                    if !last_full && full {
                        let (x, y, z) = cyc(a, b, c);
                        consumer(negative, x, y, z);
                    }
                    if last_full && !full {
                        let (x, y, z) = cyc(a, b, c - 1);
                        consumer(positive, x, y, z);
                    }
                    last_full = full;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitset_matches_java_semantics() {
        let mut b = BitSet::default();
        b.set(3);
        b.set(64);
        b.set(70);
        assert!(b.get(3) && b.get(64) && !b.get(4) && !b.get(1000));
        assert_eq!(b.next_clear_bit(3), 4);
        assert_eq!(b.next_clear_bit(64), 65);
        assert_eq!(b.next_clear_bit(500), 500);
        b.clear_range(2, 66);
        assert!(!b.get(3) && !b.get(64) && b.get(70));
        b.clear_range(70, 71);
        assert!(b.is_empty() && b.words().is_empty());
        for i in 0..128 {
            b.set(i);
        }
        assert_eq!(b.next_clear_bit(0), 128);
        b.clear_range(0, 128);
        assert!(b.is_empty());
    }
}
