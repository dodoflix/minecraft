//! `net.minecraft.world.level.levelgen.structure.BoundingBox`: inclusive integer box.
//!
//! Vanilla logs ("Invalid bounding box data") before normalising inverted bounds; that diagnostic is not reproduced.
use crate::direction::Direction;
use crate::pos::{BlockPos, ChunkPos, SectionPos, Vec3i};
use std::fmt;
use std::hash::{Hash, Hasher};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct BoundingBox {
    min_x: i32,
    min_y: i32,
    min_z: i32,
    max_x: i32,
    max_y: i32,
    max_z: i32,
}

impl BoundingBox {
    /// `new BoundingBox(int x6)`: inverted axes are swapped.
    pub const fn new(
        min_x: i32,
        min_y: i32,
        min_z: i32,
        max_x: i32,
        max_y: i32,
        max_z: i32,
    ) -> BoundingBox {
        if max_x < min_x || max_y < min_y || max_z < min_z {
            BoundingBox {
                min_x: if min_x < max_x { min_x } else { max_x },
                min_y: if min_y < max_y { min_y } else { max_y },
                min_z: if min_z < max_z { min_z } else { max_z },
                max_x: if min_x > max_x { min_x } else { max_x },
                max_y: if min_y > max_y { min_y } else { max_y },
                max_z: if min_z > max_z { min_z } else { max_z },
            }
        } else {
            BoundingBox {
                min_x,
                min_y,
                min_z,
                max_x,
                max_y,
                max_z,
            }
        }
    }

    /// `new BoundingBox(BlockPos)`
    pub const fn from_block_pos(content: &BlockPos) -> BoundingBox {
        BoundingBox::new(
            content.x(),
            content.y(),
            content.z(),
            content.x(),
            content.y(),
            content.z(),
        )
    }

    pub fn from_corners(pos0: &Vec3i, pos1: &Vec3i) -> BoundingBox {
        BoundingBox::new(
            pos0.x.min(pos1.x),
            pos0.y.min(pos1.y),
            pos0.z.min(pos1.z),
            pos0.x.max(pos1.x),
            pos0.y.max(pos1.y),
            pos0.z.max(pos1.z),
        )
    }

    pub const fn infinite() -> BoundingBox {
        BoundingBox::new(i32::MIN, i32::MIN, i32::MIN, i32::MAX, i32::MAX, i32::MAX)
    }

    /// `orientBox`: `Up`/`Down` take vanilla's `default` branch (same as `South`).
    pub const fn orient_box(
        foot_x: i32,
        foot_y: i32,
        foot_z: i32,
        off_x: i32,
        off_y: i32,
        off_z: i32,
        width: i32,
        height: i32,
        depth: i32,
        direction: Direction,
    ) -> BoundingBox {
        match direction {
            Direction::North => BoundingBox::new(
                foot_x.wrapping_add(off_x),
                foot_y.wrapping_add(off_y),
                foot_z
                    .wrapping_sub(depth)
                    .wrapping_add(1)
                    .wrapping_add(off_z),
                foot_x
                    .wrapping_add(width)
                    .wrapping_sub(1)
                    .wrapping_add(off_x),
                foot_y
                    .wrapping_add(height)
                    .wrapping_sub(1)
                    .wrapping_add(off_y),
                foot_z.wrapping_add(off_z),
            ),
            Direction::West => BoundingBox::new(
                foot_x
                    .wrapping_sub(depth)
                    .wrapping_add(1)
                    .wrapping_add(off_z),
                foot_y.wrapping_add(off_y),
                foot_z.wrapping_add(off_x),
                foot_x.wrapping_add(off_z),
                foot_y
                    .wrapping_add(height)
                    .wrapping_sub(1)
                    .wrapping_add(off_y),
                foot_z
                    .wrapping_add(width)
                    .wrapping_sub(1)
                    .wrapping_add(off_x),
            ),
            Direction::East => BoundingBox::new(
                foot_x.wrapping_add(off_z),
                foot_y.wrapping_add(off_y),
                foot_z.wrapping_add(off_x),
                foot_x
                    .wrapping_add(depth)
                    .wrapping_sub(1)
                    .wrapping_add(off_z),
                foot_y
                    .wrapping_add(height)
                    .wrapping_sub(1)
                    .wrapping_add(off_y),
                foot_z
                    .wrapping_add(width)
                    .wrapping_sub(1)
                    .wrapping_add(off_x),
            ),
            _ => BoundingBox::new(
                foot_x.wrapping_add(off_x),
                foot_y.wrapping_add(off_y),
                foot_z.wrapping_add(off_z),
                foot_x
                    .wrapping_add(width)
                    .wrapping_sub(1)
                    .wrapping_add(off_x),
                foot_y
                    .wrapping_add(height)
                    .wrapping_sub(1)
                    .wrapping_add(off_y),
                foot_z
                    .wrapping_add(depth)
                    .wrapping_sub(1)
                    .wrapping_add(off_z),
            ),
        }
    }

    pub fn intersecting_chunks(&self) -> impl Iterator<Item = ChunkPos> + use<> {
        let min_chunk_x = SectionPos::block_to_section_coord_i32(self.min_x);
        let min_chunk_z = SectionPos::block_to_section_coord_i32(self.min_z);
        let max_chunk_x = SectionPos::block_to_section_coord_i32(self.max_x);
        let max_chunk_z = SectionPos::block_to_section_coord_i32(self.max_z);
        ChunkPos::range_closed_between(
            &ChunkPos::new(min_chunk_x, min_chunk_z),
            &ChunkPos::new(max_chunk_x, max_chunk_z),
        )
    }

    pub const fn intersects(&self, other: &BoundingBox) -> bool {
        self.intersects_xyz(
            other.min_x,
            other.min_y,
            other.min_z,
            other.max_x,
            other.max_y,
            other.max_z,
        )
    }

    /// `intersects(int x6)`
    pub const fn intersects_xyz(
        &self,
        min_x: i32,
        min_y: i32,
        min_z: i32,
        max_x: i32,
        max_y: i32,
        max_z: i32,
    ) -> bool {
        self.max_x >= min_x
            && self.min_x <= max_x
            && self.max_z >= min_z
            && self.min_z <= max_z
            && self.max_y >= min_y
            && self.min_y <= max_y
    }

    /// `intersects(int minX, int minZ, int maxX, int maxZ)`
    pub const fn intersects_xz(&self, min_x: i32, min_z: i32, max_x: i32, max_z: i32) -> bool {
        self.max_x >= min_x && self.min_x <= max_x && self.max_z >= min_z && self.min_z <= max_z
    }

    pub fn encapsulating_positions(
        positions: impl IntoIterator<Item = BlockPos>,
    ) -> Option<BoundingBox> {
        let mut iter = positions.into_iter();
        let mut result = BoundingBox::from_block_pos(&iter.next()?);
        iter.for_each(|pos| {
            result.encapsulate_pos(&pos);
        });
        Some(result)
    }

    pub fn encapsulating_boxes(
        boxes: impl IntoIterator<Item = BoundingBox>,
    ) -> Option<BoundingBox> {
        let mut iter = boxes.into_iter();
        let mut result = iter.next()?;
        iter.for_each(|b| {
            result.encapsulate(&b);
        });
        Some(result)
    }

    /// `encapsulate(BoundingBox)`: grows `self` in place.
    pub fn encapsulate(&mut self, other: &BoundingBox) -> &mut Self {
        self.min_x = self.min_x.min(other.min_x);
        self.min_y = self.min_y.min(other.min_y);
        self.min_z = self.min_z.min(other.min_z);
        self.max_x = self.max_x.max(other.max_x);
        self.max_y = self.max_y.max(other.max_y);
        self.max_z = self.max_z.max(other.max_z);
        self
    }

    /// `BoundingBox.encapsulating(a, b)`
    pub fn encapsulating(a: &BoundingBox, b: &BoundingBox) -> BoundingBox {
        BoundingBox::new(
            a.min_x.min(b.min_x),
            a.min_y.min(b.min_y),
            a.min_z.min(b.min_z),
            a.max_x.max(b.max_x),
            a.max_y.max(b.max_y),
            a.max_z.max(b.max_z),
        )
    }

    /// `encapsulate(BlockPos)`: grows `self` in place.
    pub fn encapsulate_pos(&mut self, pos: &BlockPos) -> &mut Self {
        self.min_x = self.min_x.min(pos.x());
        self.min_y = self.min_y.min(pos.y());
        self.min_z = self.min_z.min(pos.z());
        self.max_x = self.max_x.max(pos.x());
        self.max_y = self.max_y.max(pos.y());
        self.max_z = self.max_z.max(pos.z());
        self
    }

    /// `move(int, int, int)`: translates `self` in place.
    pub const fn move_(&mut self, dx: i32, dy: i32, dz: i32) -> &mut Self {
        self.min_x = self.min_x.wrapping_add(dx);
        self.min_y = self.min_y.wrapping_add(dy);
        self.min_z = self.min_z.wrapping_add(dz);
        self.max_x = self.max_x.wrapping_add(dx);
        self.max_y = self.max_y.wrapping_add(dy);
        self.max_z = self.max_z.wrapping_add(dz);
        self
    }

    /// `move(Vec3i)`
    pub const fn move_vec(&mut self, amount: &Vec3i) -> &mut Self {
        self.move_(amount.x, amount.y, amount.z)
    }

    pub const fn moved(&self, dx: i32, dy: i32, dz: i32) -> BoundingBox {
        BoundingBox::new(
            self.min_x.wrapping_add(dx),
            self.min_y.wrapping_add(dy),
            self.min_z.wrapping_add(dz),
            self.max_x.wrapping_add(dx),
            self.max_y.wrapping_add(dy),
            self.max_z.wrapping_add(dz),
        )
    }

    pub const fn inflated_by(&self, amount_to_add_all_directions: i32) -> BoundingBox {
        self.inflated_by_xyz(
            amount_to_add_all_directions,
            amount_to_add_all_directions,
            amount_to_add_all_directions,
        )
    }

    /// `inflatedBy(int, int, int)`
    pub const fn inflated_by_xyz(
        &self,
        inflate_x: i32,
        inflate_y: i32,
        inflate_z: i32,
    ) -> BoundingBox {
        BoundingBox::new(
            self.min_x.wrapping_sub(inflate_x),
            self.min_y.wrapping_sub(inflate_y),
            self.min_z.wrapping_sub(inflate_z),
            self.max_x.wrapping_add(inflate_x),
            self.max_y.wrapping_add(inflate_y),
            self.max_z.wrapping_add(inflate_z),
        )
    }

    pub const fn is_inside(&self, pos: &Vec3i) -> bool {
        self.is_inside_xyz(pos.x, pos.y, pos.z)
    }

    /// `isInside(int, int, int)`
    pub const fn is_inside_xyz(&self, x: i32, y: i32, z: i32) -> bool {
        x >= self.min_x
            && x <= self.max_x
            && z >= self.min_z
            && z <= self.max_z
            && y >= self.min_y
            && y <= self.max_y
    }

    pub const fn get_length(&self) -> Vec3i {
        Vec3i::new(
            self.max_x.wrapping_sub(self.min_x),
            self.max_y.wrapping_sub(self.min_y),
            self.max_z.wrapping_sub(self.min_z),
        )
    }

    pub const fn get_x_span(&self) -> i32 {
        self.max_x.wrapping_sub(self.min_x).wrapping_add(1)
    }

    pub const fn get_y_span(&self) -> i32 {
        self.max_y.wrapping_sub(self.min_y).wrapping_add(1)
    }

    pub const fn get_z_span(&self) -> i32 {
        self.max_z.wrapping_sub(self.min_z).wrapping_add(1)
    }

    /// Java `int` division truncates toward zero (matters only for overflowed spans).
    pub const fn get_center(&self) -> BlockPos {
        BlockPos::new(
            self.min_x.wrapping_add(self.get_x_span().wrapping_div(2)),
            self.min_y.wrapping_add(self.get_y_span().wrapping_div(2)),
            self.min_z.wrapping_add(self.get_z_span().wrapping_div(2)),
        )
    }

    pub fn for_all_corners(&self, mut consumer: impl FnMut(BlockPos)) {
        consumer(BlockPos::new(self.max_x, self.max_y, self.max_z));
        consumer(BlockPos::new(self.min_x, self.max_y, self.max_z));
        consumer(BlockPos::new(self.max_x, self.min_y, self.max_z));
        consumer(BlockPos::new(self.min_x, self.min_y, self.max_z));
        consumer(BlockPos::new(self.max_x, self.max_y, self.min_z));
        consumer(BlockPos::new(self.min_x, self.max_y, self.min_z));
        consumer(BlockPos::new(self.max_x, self.min_y, self.min_z));
        consumer(BlockPos::new(self.min_x, self.min_y, self.min_z));
    }

    pub const fn min_x(&self) -> i32 {
        self.min_x
    }

    pub const fn min_y(&self) -> i32 {
        self.min_y
    }

    pub const fn min_z(&self) -> i32 {
        self.min_z
    }

    pub const fn max_x(&self) -> i32 {
        self.max_x
    }

    pub const fn max_y(&self) -> i32 {
        self.max_y
    }

    pub const fn max_z(&self) -> i32 {
        self.max_z
    }

    /// `Objects.hash(minX, minY, minZ, maxX, maxY, maxZ)`
    pub const fn hash_code(&self) -> i32 {
        let mut h = 1i32;
        let fields = [
            self.min_x, self.min_y, self.min_z, self.max_x, self.max_y, self.max_z,
        ];
        let mut i = 0;
        while i < fields.len() {
            h = h.wrapping_mul(31).wrapping_add(fields[i]);
            i += 1;
        }
        h
    }
}

impl Hash for BoundingBox {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_i32(self.hash_code());
    }
}

impl fmt::Display for BoundingBox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BoundingBox{{minX={}, minY={}, minZ={}, maxX={}, maxY={}, maxZ={}}}",
            self.min_x, self.min_y, self.min_z, self.max_x, self.max_y, self.max_z
        )
    }
}

impl fmt::Debug for BoundingBox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
