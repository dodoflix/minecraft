//! `net.minecraft.core.SectionPos`
use super::block_pos::BlockPos;
use super::chunk_pos::ChunkPos;
use super::cursor3d::Cursor3D;
use super::vec3i::{Vec3i, fmt_named};
use crate::direction::Direction;
use crate::math::mth;
use crate::phys::Vec3;
use std::fmt;
use std::ops::Deref;

/// Section coordinates (block coordinate >> 4); a `Vec3i` newtype like vanilla's subclass.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct SectionPos(Vec3i);

const Y_OFFSET: i32 = 0;
const Z_OFFSET: i32 = 20;
const X_OFFSET: i32 = 42;
const PACKED_X_MASK: i64 = 4194303;
const PACKED_Y_MASK: i64 = 1048575;
const PACKED_Z_MASK: i64 = 4194303;
const RELATIVE_X_SHIFT: u32 = 8;
const RELATIVE_Y_SHIFT: u32 = 0;
const RELATIVE_Z_SHIFT: u32 = 4;

impl Deref for SectionPos {
    type Target = Vec3i;

    fn deref(&self) -> &Vec3i {
        &self.0
    }
}

impl fmt::Display for SectionPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_named(f, "SectionPos", self.0.x, self.0.y, self.0.z)
    }
}

impl fmt::Debug for SectionPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl SectionPos {
    pub const SECTION_BITS: i32 = 4;
    pub const SECTION_SIZE: i32 = 16;
    pub const SECTION_BLOCK_COUNT: i32 = 4096;
    pub const SECTION_MASK: i32 = 15;
    pub const SECTION_HALF_SIZE: i32 = 8;
    pub const SECTION_MAX_INDEX: i32 = 15;

    pub const fn of(x: i32, y: i32, z: i32) -> SectionPos {
        SectionPos(Vec3i::new(x, y, z))
    }

    /// `of(BlockPos)`
    pub const fn of_block_pos(pos: &BlockPos) -> SectionPos {
        Self::of(
            Self::block_to_section_coord_i32(pos.x()),
            Self::block_to_section_coord_i32(pos.y()),
            Self::block_to_section_coord_i32(pos.z()),
        )
    }

    /// `of(ChunkPos, int sectionY)`
    pub const fn of_chunk(pos: &ChunkPos, section_y: i32) -> SectionPos {
        Self::of(pos.x, section_y, pos.z)
    }

    /// `of(Position)`
    pub fn of_position(pos: &Vec3) -> SectionPos {
        Self::of(
            Self::block_to_section_coord_f64(pos.x),
            Self::block_to_section_coord_f64(pos.y),
            Self::block_to_section_coord_f64(pos.z),
        )
    }

    /// `of(long sectionNode)`
    pub const fn of_long(section_node: i64) -> SectionPos {
        Self::of(Self::x_from_long(section_node), Self::y_from_long(section_node), Self::z_from_long(section_node))
    }

    /// `SectionPos.offset(long, Direction)`
    pub fn offset_long_direction(section_node: i64, offset: Direction) -> i64 {
        Self::offset_long(section_node, offset.get_step_x(), offset.get_step_y(), offset.get_step_z())
    }

    /// `SectionPos.offset(long, int, int, int)`
    pub const fn offset_long(section_node: i64, step_x: i32, step_y: i32, step_z: i32) -> i64 {
        Self::as_long_xyz(
            Self::x_from_long(section_node).wrapping_add(step_x),
            Self::y_from_long(section_node).wrapping_add(step_y),
            Self::z_from_long(section_node).wrapping_add(step_z),
        )
    }

    pub fn pos_to_section_coord(pos: f64) -> i32 {
        Self::block_to_section_coord_i32(mth::floor_f64(pos))
    }

    /// `blockToSectionCoord(int)`
    pub const fn block_to_section_coord_i32(block_coord: i32) -> i32 {
        block_coord >> 4
    }

    /// `blockToSectionCoord(double)`
    pub fn block_to_section_coord_f64(coord: f64) -> i32 {
        mth::floor_f64(coord) >> 4
    }

    pub const fn section_relative(block_coord: i32) -> i32 {
        block_coord & 15
    }

    pub const fn section_relative_pos(pos: &BlockPos) -> i16 {
        let x = Self::section_relative(pos.x());
        let y = Self::section_relative(pos.y());
        let z = Self::section_relative(pos.z());
        (x << RELATIVE_X_SHIFT | z << RELATIVE_Z_SHIFT | y << RELATIVE_Y_SHIFT) as i16
    }

    // Java widens the `short` to a sign-extended `int` before `>>>`.
    pub const fn section_relative_x(relative: i16) -> i32 {
        (relative as i32 as u32 >> RELATIVE_X_SHIFT & 15) as i32
    }

    pub const fn section_relative_y(relative: i16) -> i32 {
        (relative as i32 as u32 >> RELATIVE_Y_SHIFT & 15) as i32
    }

    pub const fn section_relative_z(relative: i16) -> i32 {
        (relative as i32 as u32 >> RELATIVE_Z_SHIFT & 15) as i32
    }

    pub const fn relative_to_block_x(&self, relative: i16) -> i32 {
        self.min_block_x().wrapping_add(Self::section_relative_x(relative))
    }

    pub const fn relative_to_block_y(&self, relative: i16) -> i32 {
        self.min_block_y().wrapping_add(Self::section_relative_y(relative))
    }

    pub const fn relative_to_block_z(&self, relative: i16) -> i32 {
        self.min_block_z().wrapping_add(Self::section_relative_z(relative))
    }

    pub const fn relative_to_block_pos(&self, relative: i16) -> BlockPos {
        BlockPos::new(self.relative_to_block_x(relative), self.relative_to_block_y(relative), self.relative_to_block_z(relative))
    }

    pub const fn section_to_block_coord(section_coord: i32) -> i32 {
        section_coord << 4
    }

    /// `sectionToBlockCoord(int, int offset)`
    pub const fn section_to_block_coord_offset(section_coord: i32, offset: i32) -> i32 {
        Self::section_to_block_coord(section_coord).wrapping_add(offset)
    }

    /// `SectionPos.x(long)`
    pub const fn x_from_long(section_node: i64) -> i32 {
        (section_node >> X_OFFSET) as i32
    }

    /// `SectionPos.y(long)`
    pub const fn y_from_long(section_node: i64) -> i32 {
        (section_node << 44 >> 44) as i32
    }

    /// `SectionPos.z(long)`
    pub const fn z_from_long(section_node: i64) -> i32 {
        (section_node << 22 >> 42) as i32
    }

    pub const fn min_block_x(&self) -> i32 {
        Self::section_to_block_coord(self.0.x)
    }

    pub const fn min_block_y(&self) -> i32 {
        Self::section_to_block_coord(self.0.y)
    }

    pub const fn min_block_z(&self) -> i32 {
        Self::section_to_block_coord(self.0.z)
    }

    pub const fn max_block_x(&self) -> i32 {
        Self::section_to_block_coord_offset(self.0.x, 15)
    }

    pub const fn max_block_y(&self) -> i32 {
        Self::section_to_block_coord_offset(self.0.y, 15)
    }

    pub const fn max_block_z(&self) -> i32 {
        Self::section_to_block_coord_offset(self.0.z, 15)
    }

    pub const fn block_to_section(block_node: i64) -> i64 {
        Self::as_long_xyz(
            Self::block_to_section_coord_i32(BlockPos::get_x(block_node)),
            Self::block_to_section_coord_i32(BlockPos::get_y(block_node)),
            Self::block_to_section_coord_i32(BlockPos::get_z(block_node)),
        )
    }

    /// `getZeroNode(int x, int z)`
    pub const fn get_zero_node_xz(x: i32, z: i32) -> i64 {
        Self::get_zero_node(Self::as_long_xyz(x, 0, z))
    }

    pub const fn get_zero_node(section_node: i64) -> i64 {
        section_node & -1048576
    }

    pub const fn section_to_chunk(section_node: i64) -> i64 {
        ChunkPos::pack_xz(Self::x_from_long(section_node), Self::z_from_long(section_node))
    }

    pub const fn origin(&self) -> BlockPos {
        BlockPos::new(Self::section_to_block_coord(self.0.x), Self::section_to_block_coord(self.0.y), Self::section_to_block_coord(self.0.z))
    }

    pub const fn center(&self) -> BlockPos {
        self.origin().offset(8, 8, 8)
    }

    pub const fn chunk(&self) -> ChunkPos {
        ChunkPos::new(self.0.x, self.0.z)
    }

    /// `SectionPos.asLong(BlockPos)`
    pub const fn as_long_block_pos(pos: &BlockPos) -> i64 {
        Self::as_long_xyz(
            Self::block_to_section_coord_i32(pos.x()),
            Self::block_to_section_coord_i32(pos.y()),
            Self::block_to_section_coord_i32(pos.z()),
        )
    }

    /// `SectionPos.asLong(int, int, int)`
    pub const fn as_long_xyz(x: i32, y: i32, z: i32) -> i64 {
        let mut node = 0i64;
        node |= (x as i64 & PACKED_X_MASK) << X_OFFSET;
        node |= (y as i64 & PACKED_Y_MASK) << Y_OFFSET;
        node | (z as i64 & PACKED_Z_MASK) << Z_OFFSET
    }

    pub const fn as_long(&self) -> i64 {
        Self::as_long_xyz(self.0.x, self.0.y, self.0.z)
    }

    pub const fn offset(&self, x: i32, y: i32, z: i32) -> SectionPos {
        SectionPos(self.0.offset(x, y, z))
    }

    pub fn blocks_inside(&self) -> impl Iterator<Item = BlockPos> + use<> {
        BlockPos::between_closed_xyz(self.min_block_x(), self.min_block_y(), self.min_block_z(), self.max_block_x(), self.max_block_y(), self.max_block_z())
    }

    pub fn cube(center: &SectionPos, radius: i32) -> impl Iterator<Item = SectionPos> + use<> {
        let (x, y, z) = (center.0.x, center.0.y, center.0.z);
        Self::between_closed_stream(
            x.wrapping_sub(radius),
            y.wrapping_sub(radius),
            z.wrapping_sub(radius),
            x.wrapping_add(radius),
            y.wrapping_add(radius),
            z.wrapping_add(radius),
        )
    }

    pub fn around_chunk(center: &ChunkPos, radius: i32, min_section: i32, max_section: i32) -> impl Iterator<Item = SectionPos> + use<> {
        Self::between_closed_stream(
            center.x.wrapping_sub(radius),
            min_section,
            center.z.wrapping_sub(radius),
            center.x.wrapping_add(radius),
            max_section,
            center.z.wrapping_add(radius),
        )
    }

    pub fn between_closed_stream(min_x: i32, min_y: i32, min_z: i32, max_x: i32, max_y: i32, max_z: i32) -> impl Iterator<Item = SectionPos> + use<> {
        let mut cursor = Cursor3D::new(min_x, min_y, min_z, max_x, max_y, max_z);
        std::iter::from_fn(move || cursor.advance().then(|| SectionPos::of(cursor.next_x(), cursor.next_y(), cursor.next_z())))
    }

    /// `aroundAndAtBlockPos(BlockPos, LongConsumer)`
    pub fn around_and_at_block_pos(block_pos: &BlockPos, section_consumer: impl FnMut(i64)) {
        Self::around_and_at_block_pos_xyz(block_pos.x(), block_pos.y(), block_pos.z(), section_consumer);
    }

    /// `aroundAndAtBlockPos(long, LongConsumer)`
    pub fn around_and_at_block_pos_long(block_pos: i64, section_consumer: impl FnMut(i64)) {
        Self::around_and_at_block_pos_xyz(BlockPos::get_x(block_pos), BlockPos::get_y(block_pos), BlockPos::get_z(block_pos), section_consumer);
    }

    /// `aroundAndAtBlockPos(int, int, int, LongConsumer)`
    pub fn around_and_at_block_pos_xyz(block_x: i32, block_y: i32, block_z: i32, mut section_consumer: impl FnMut(i64)) {
        let min_x = Self::block_to_section_coord_i32(block_x.wrapping_sub(1));
        let max_x = Self::block_to_section_coord_i32(block_x.wrapping_add(1));
        let min_y = Self::block_to_section_coord_i32(block_y.wrapping_sub(1));
        let max_y = Self::block_to_section_coord_i32(block_y.wrapping_add(1));
        let min_z = Self::block_to_section_coord_i32(block_z.wrapping_sub(1));
        let max_z = Self::block_to_section_coord_i32(block_z.wrapping_add(1));
        if min_x == max_x && min_y == max_y && min_z == max_z {
            section_consumer(Self::as_long_xyz(min_x, min_y, min_z));
        } else {
            for section_x in min_x..=max_x {
                for section_y in min_y..=max_y {
                    for section_z in min_z..=max_z {
                        section_consumer(Self::as_long_xyz(section_x, section_y, section_z));
                    }
                }
            }
        }
    }
}
