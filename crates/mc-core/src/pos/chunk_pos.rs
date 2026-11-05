//! `net.minecraft.world.level.ChunkPos`
use super::block_pos::BlockPos;
use super::section_pos::SectionPos;
use crate::math::mth;
use std::fmt;
use std::hash::{Hash, Hasher};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

const REGION_BITS: i32 = 5;
const REGION_MASK: i32 = 31;
const HASH_A: i32 = 1664525;
const HASH_C: i32 = 1013904223;
const HASH_Z_XOR: i32 = -559038737;

impl ChunkPos {
    pub const ZERO: ChunkPos = ChunkPos::new(0, 0);
    pub const REGION_SIZE: i32 = 32;
    pub const REGION_MAX_INDEX: i32 = 31;
    /// `ChunkPyramid.MAX_CHUNK_COORDINATE_VALUE` (section of `BlockPos.MAX_HORIZONTAL_COORDINATE` minus the
    /// pyramid safety margin; value taken from the vanilla oracle, verified in `tests/golden/pos.rs`).
    pub const MAX_CHUNK_COORDINATE_VALUE: i32 = 2097061;
    pub const INVALID_CHUNK_POS: i64 = ChunkPos::pack_xz(1875066, 1875066);

    pub const fn new(x: i32, z: i32) -> ChunkPos {
        ChunkPos { x, z }
    }

    pub const fn containing(pos: &BlockPos) -> ChunkPos {
        ChunkPos::new(SectionPos::block_to_section_coord_i32(pos.x()), SectionPos::block_to_section_coord_i32(pos.z()))
    }

    /// `ChunkPos.unpack(long)`
    pub const fn unpack(key: i64) -> ChunkPos {
        ChunkPos::new(key as i32, (key >> 32) as i32)
    }

    pub const fn min_from_region(region_x: i32, region_z: i32) -> ChunkPos {
        ChunkPos::new(region_x << REGION_BITS, region_z << REGION_BITS)
    }

    pub const fn max_from_region(region_x: i32, region_z: i32) -> ChunkPos {
        ChunkPos::new((region_x << REGION_BITS).wrapping_add(31), (region_z << REGION_BITS).wrapping_add(31))
    }

    pub fn is_valid(&self) -> bool {
        Self::is_valid_xz(self.x, self.z)
    }

    pub fn is_valid_xz(x: i32, z: i32) -> bool {
        mth::abs_max_i32(x, z) <= Self::MAX_CHUNK_COORDINATE_VALUE
    }

    /// `pack()`
    pub const fn pack(&self) -> i64 {
        Self::pack_xz(self.x, self.z)
    }

    /// `ChunkPos.pack(int, int)`
    pub const fn pack_xz(x: i32, z: i32) -> i64 {
        (x as i64 & 0xFFFF_FFFF) | ((z as i64 & 0xFFFF_FFFF) << 32)
    }

    pub const fn from_section_node(section_node: i64) -> i64 {
        Self::pack_xz(SectionPos::x_from_long(section_node), SectionPos::z_from_long(section_node))
    }

    /// `ChunkPos.pack(BlockPos)`
    pub const fn pack_block_pos(pos: &BlockPos) -> i64 {
        Self::pack_xz(SectionPos::block_to_section_coord_i32(pos.x()), SectionPos::block_to_section_coord_i32(pos.z()))
    }

    /// `ChunkPos.getX(long)`
    pub const fn get_x(pos: i64) -> i32 {
        (pos & 0xFFFF_FFFF) as i32
    }

    /// `ChunkPos.getZ(long)`
    pub const fn get_z(pos: i64) -> i32 {
        ((pos as u64 >> 32) & 0xFFFF_FFFF) as i32
    }

    /// `hashCode()` / `ChunkPos.hash(int, int)`
    pub const fn hash_xz(x: i32, z: i32) -> i32 {
        let x_transform = HASH_A.wrapping_mul(x).wrapping_add(HASH_C);
        let z_transform = HASH_A.wrapping_mul(z ^ HASH_Z_XOR).wrapping_add(HASH_C);
        x_transform ^ z_transform
    }

    pub const fn hash_code(&self) -> i32 {
        Self::hash_xz(self.x, self.z)
    }

    pub const fn get_middle_block_x(&self) -> i32 {
        self.get_block_x(8)
    }

    pub const fn get_middle_block_z(&self) -> i32 {
        self.get_block_z(8)
    }

    pub const fn get_min_block_x(&self) -> i32 {
        SectionPos::section_to_block_coord(self.x)
    }

    pub const fn get_min_block_z(&self) -> i32 {
        SectionPos::section_to_block_coord(self.z)
    }

    pub const fn get_max_block_x(&self) -> i32 {
        self.get_block_x(15)
    }

    pub const fn get_max_block_z(&self) -> i32 {
        self.get_block_z(15)
    }

    pub const fn get_region_x(&self) -> i32 {
        self.x >> REGION_BITS
    }

    pub const fn get_region_z(&self) -> i32 {
        self.z >> REGION_BITS
    }

    /// `ChunkPos.getRegionX(long)`
    pub const fn get_region_x_from_long(pos: i64) -> i32 {
        Self::get_x(pos) >> REGION_BITS
    }

    /// `ChunkPos.getRegionZ(long)`
    pub const fn get_region_z_from_long(pos: i64) -> i32 {
        Self::get_z(pos) >> REGION_BITS
    }

    pub const fn get_region_local_x(&self) -> i32 {
        self.x & REGION_MASK
    }

    pub const fn get_region_local_z(&self) -> i32 {
        self.z & REGION_MASK
    }

    pub const fn get_block_at(&self, x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(self.get_block_x(x), y, self.get_block_z(z))
    }

    pub const fn get_block_x(&self, offset: i32) -> i32 {
        SectionPos::section_to_block_coord_offset(self.x, offset)
    }

    pub const fn get_block_z(&self, offset: i32) -> i32 {
        SectionPos::section_to_block_coord_offset(self.z, offset)
    }

    pub const fn get_middle_block_position(&self, y: i32) -> BlockPos {
        BlockPos::new(self.get_middle_block_x(), y, self.get_middle_block_z())
    }

    pub const fn contains(&self, pos: &BlockPos) -> bool {
        pos.x() >= self.get_min_block_x()
            && pos.z() >= self.get_min_block_z()
            && pos.x() <= self.get_max_block_x()
            && pos.z() <= self.get_max_block_z()
    }

    pub const fn get_world_position(&self) -> BlockPos {
        BlockPos::new(self.get_min_block_x(), 0, self.get_min_block_z())
    }

    pub fn get_chessboard_distance(&self, pos: &ChunkPos) -> i32 {
        self.get_chessboard_distance_xz(pos.x, pos.z)
    }

    pub fn get_chessboard_distance_xz(&self, x: i32, z: i32) -> i32 {
        mth::chessboard_distance(x, z, self.x, self.z)
    }

    pub const fn distance_squared(&self, pos: &ChunkPos) -> i32 {
        self.distance_squared_xz(pos.x, pos.z)
    }

    /// `distanceSquared(long)`
    pub const fn distance_squared_long(&self, pos: i64) -> i32 {
        self.distance_squared_xz(Self::get_x(pos), Self::get_z(pos))
    }

    const fn distance_squared_xz(&self, x: i32, z: i32) -> i32 {
        let delta_x = x.wrapping_sub(self.x);
        let delta_z = z.wrapping_sub(self.z);
        delta_x.wrapping_mul(delta_x).wrapping_add(delta_z.wrapping_mul(delta_z))
    }

    /// `rangeClosed(ChunkPos center, int radius)`
    pub fn range_closed(center: &ChunkPos, radius: i32) -> impl Iterator<Item = ChunkPos> + use<> {
        Self::range_closed_between(
            &ChunkPos::new(center.x.wrapping_sub(radius), center.z.wrapping_sub(radius)),
            &ChunkPos::new(center.x.wrapping_add(radius), center.z.wrapping_add(radius)),
        )
    }

    /// `rangeClosed(ChunkPos from, ChunkPos to)`: x fastest, then z.
    pub fn range_closed_between(from: &ChunkPos, to: &ChunkPos) -> impl Iterator<Item = ChunkPos> + use<> {
        let (from, to) = (*from, *to);
        let x_diff = if from.x < to.x { 1 } else { -1 };
        let z_diff = if from.z < to.z { 1 } else { -1 };
        let mut pos: Option<ChunkPos> = None;
        std::iter::from_fn(move || {
            let next = match pos {
                None => from,
                Some(p) if p.x == to.x => {
                    if p.z == to.z {
                        return None;
                    }
                    ChunkPos::new(from.x, p.z.wrapping_add(z_diff))
                }
                Some(p) => ChunkPos::new(p.x.wrapping_add(x_diff), p.z),
            };
            pos = Some(next);
            Some(next)
        })
        .fuse()
    }
}

impl Hash for ChunkPos {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_i32(self.hash_code());
    }
}

/// `[x, z]`
impl fmt::Display for ChunkPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}, {}]", self.x, self.z)
    }
}

impl fmt::Debug for ChunkPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
