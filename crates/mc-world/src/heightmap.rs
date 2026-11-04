//! `net.minecraft.world.level.levelgen.Heightmap`: per-column "first available Y" maps stored as a
//! [`SimpleBitStorage`] of `ceillog2(height + 1)`-bit values (`y - min_y`), plus the `Heightmap.Types` enum.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use mc_core::bits::{BitStorage, SimpleBitStorage};
use mc_core::math::mth;
use mc_core::pos::ChunkPos;
use mc_data::block::{BLOCK_COUNT, BlockState};
use mc_data::{Block, tags};

/// `EnumMap<Heightmap.Types, Heightmap>` (ordinal iteration order).
pub type HeightmapMap = BTreeMap<HeightmapType, Heightmap>;

/// The view of a chunk that heightmaps need (`ChunkAccess.getMinY/getHeight/getHighestSectionPosition/getBlockState`).
pub trait HeightmapChunk {
    fn min_y(&self) -> i32;
    fn height(&self) -> i32;
    /// `ChunkAccess.getHighestSectionPosition`: block Y of the highest section that is not `hasOnlyAir`,
    /// or `min_y()` when every section is empty.
    fn highest_section_position(&self) -> i32;
    /// Chunk-local `x`/`z` (0..16), absolute `y`.
    fn get_block_state(&self, x: i32, y: i32, z: i32) -> BlockState;
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum Usage {
    Worldgen,
    LiveWorld,
    Client,
}

/// `Heightmap.Types`, in ordinal order.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum HeightmapType {
    WorldSurfaceWg,
    WorldSurface,
    OceanFloorWg,
    OceanFloor,
    MotionBlocking,
    MotionBlockingNoLeaves,
}

fn block_table(tag: &str) -> Box<[bool]> {
    let set = tags::get::<Block>(tag).unwrap_or_else(|| panic!("vanilla block tag {tag} missing from mc-data"));
    let mut table = vec![false; BLOCK_COUNT];
    for block in set.iter() {
        table[block.id() as usize] = true;
    }
    table.into_boxed_slice()
}

/// `state.is(BlockTags.BLOCKS_MOTION_IN_HEIGHTMAP)`.
fn blocks_motion(state: BlockState) -> bool {
    static TABLE: LazyLock<Box<[bool]>> = LazyLock::new(|| block_table("blocks_motion_in_heightmap"));
    TABLE[state.block().id() as usize]
}

/// `state.is(BlockTags.BLOCKS_MOTION_IN_HEIGHTMAP_NO_LEAVES)`.
fn blocks_motion_no_leaves(state: BlockState) -> bool {
    static TABLE: LazyLock<Box<[bool]>> = LazyLock::new(|| block_table("blocks_motion_in_heightmap_no_leaves"));
    TABLE[state.block().id() as usize]
}

impl HeightmapType {
    /// `Types.values()`.
    pub const ALL: [HeightmapType; 6] = [
        HeightmapType::WorldSurfaceWg,
        HeightmapType::WorldSurface,
        HeightmapType::OceanFloorWg,
        HeightmapType::OceanFloor,
        HeightmapType::MotionBlocking,
        HeightmapType::MotionBlockingNoLeaves,
    ];

    pub const fn id(self) -> u8 {
        self as u8
    }

    /// `ByIdMap.continuous(.., ZERO)`: out-of-range ids (including negative) map to `WORLD_SURFACE_WG`.
    pub fn from_id(id: i32) -> HeightmapType {
        usize::try_from(id).ok().and_then(|i| Self::ALL.get(i).copied()).unwrap_or(HeightmapType::WorldSurfaceWg)
    }

    pub const fn serialization_key(self) -> &'static str {
        match self {
            HeightmapType::WorldSurfaceWg => "WORLD_SURFACE_WG",
            HeightmapType::WorldSurface => "WORLD_SURFACE",
            HeightmapType::OceanFloorWg => "OCEAN_FLOOR_WG",
            HeightmapType::OceanFloor => "OCEAN_FLOOR",
            HeightmapType::MotionBlocking => "MOTION_BLOCKING",
            HeightmapType::MotionBlockingNoLeaves => "MOTION_BLOCKING_NO_LEAVES",
        }
    }

    /// `Types.CODEC` string decode (exact, case-sensitive).
    pub fn from_serialization_key(key: &str) -> Option<HeightmapType> {
        Self::ALL.into_iter().find(|t| t.serialization_key() == key)
    }

    const fn usage(self) -> Usage {
        match self {
            HeightmapType::WorldSurfaceWg | HeightmapType::OceanFloorWg => Usage::Worldgen,
            HeightmapType::OceanFloor => Usage::LiveWorld,
            HeightmapType::WorldSurface | HeightmapType::MotionBlocking | HeightmapType::MotionBlockingNoLeaves => {
                Usage::Client
            }
        }
    }

    pub fn send_to_client(self) -> bool {
        self.usage() == Usage::Client
    }

    pub fn keep_after_worldgen(self) -> bool {
        self.usage() != Usage::Worldgen
    }

    /// `Types.isOpaque().test(state)`.
    pub fn is_opaque(self, state: BlockState) -> bool {
        match self {
            HeightmapType::WorldSurfaceWg | HeightmapType::WorldSurface => !state.is_air(),
            HeightmapType::OceanFloorWg | HeightmapType::OceanFloor => blocks_motion(state),
            HeightmapType::MotionBlocking => blocks_motion(state) || !state.get_fluid_state().is_empty(),
            HeightmapType::MotionBlockingNoLeaves => {
                blocks_motion_no_leaves(state) || !state.get_fluid_state().is_empty()
            }
        }
    }
}

/// `setRawData` was given a `long[]` of the wrong length; the heightmap is left untouched and vanilla re-primes it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SizeMismatch {
    pub expected: usize,
    pub got: usize,
}

impl SizeMismatch {
    /// Vanilla's `LOGGER.warn` text.
    pub fn message(&self, chunk_pos: ChunkPos) -> String {
        format!(
            "Ignoring heightmap data for chunk {chunk_pos}, size does not match; expected: {}, got: {}",
            self.expected, self.got
        )
    }
}

#[derive(Clone, Debug)]
pub struct Heightmap {
    data: SimpleBitStorage,
    min_y: i32,
    ty: HeightmapType,
}

impl Heightmap {
    /// `new Heightmap(chunk, type)` for a chunk with the given `getMinY()` / `getHeight()`.
    pub fn new(min_y: i32, height: i32, ty: HeightmapType) -> Heightmap {
        Heightmap { data: SimpleBitStorage::new(mth::ceillog2(height + 1), 256), min_y, ty }
    }

    pub fn heightmap_type(&self) -> HeightmapType {
        self.ty
    }

    /// `Heightmap.primeHeightmaps(chunk, types)`; `types` must be distinct (vanilla: an `EnumSet`, visited in ordinal order).
    pub fn prime_heightmaps(chunk: &impl HeightmapChunk, heightmaps: &mut HeightmapMap, types: &[HeightmapType]) {
        if types.is_empty() {
            return;
        }
        // getOrCreateHeightmapUnprimed for each type; heightmaps are taken out so the chunk can be read meanwhile.
        let mut all: Vec<Heightmap> = types
            .iter()
            .map(|&ty| heightmaps.remove(&ty).unwrap_or_else(|| Heightmap::new(chunk.min_y(), chunk.height(), ty)))
            .collect();

        let mut remaining: Vec<usize> = Vec::with_capacity(all.len());
        let highest_section_position = chunk.highest_section_position() + 16;
        let min_y = chunk.min_y();
        for x in 0..16 {
            for z in 0..16 {
                remaining.clear();
                remaining.extend(0..all.len());
                for y in (min_y..highest_section_position).rev() {
                    let state = chunk.get_block_state(x, y, z);
                    if state.block() != Block::AIR {
                        remaining.retain(|&i| {
                            if all[i].ty.is_opaque(state) {
                                all[i].set_height(x, z, y + 1);
                                false
                            } else {
                                true
                            }
                        });
                        if remaining.is_empty() {
                            break;
                        }
                    }
                }
            }
        }

        for heightmap in all {
            heightmaps.entry(heightmap.ty).or_insert(heightmap);
        }
    }

    /// `Heightmap.update(x, y, z, state)`; `local_y` is an absolute Y despite the name.
    pub fn update(&mut self, local_x: i32, local_y: i32, local_z: i32, state: BlockState, chunk: &impl HeightmapChunk) -> bool {
        let first_available = self.get_first_available(local_x, local_z);
        if local_y <= first_available - 2 {
            return false;
        }
        if self.ty.is_opaque(state) {
            if local_y >= first_available {
                self.set_height(local_x, local_z, local_y + 1);
                return true;
            }
        } else if first_available - 1 == local_y {
            for y in (self.min_y..local_y).rev() {
                if self.ty.is_opaque(chunk.get_block_state(local_x, y, local_z)) {
                    self.set_height(local_x, local_z, y + 1);
                    return true;
                }
            }
            self.set_height(local_x, local_z, self.min_y);
            return true;
        }
        false
    }

    pub fn get_first_available(&self, x: i32, z: i32) -> i32 {
        self.first_available_at(Self::get_index(x, z))
    }

    pub fn get_highest_taken(&self, x: i32, z: i32) -> i32 {
        self.get_first_available(x, z) - 1
    }

    fn first_available_at(&self, index: usize) -> i32 {
        self.data.get(index) + self.min_y
    }

    pub fn set_height(&mut self, x: i32, z: i32, height: i32) {
        self.data.set(Self::get_index(x, z), height - self.min_y);
    }

    /// `Heightmap.setRawData` without the re-prime: on a length mismatch nothing is copied and the caller must
    /// log [`SizeMismatch::message`] and call [`Heightmap::prime_heightmaps`] for this type.
    pub fn set_raw_data(&mut self, data: &[i64]) -> Result<(), SizeMismatch> {
        let expected = self.data.get_raw().len();
        if expected != data.len() {
            return Err(SizeMismatch { expected, got: data.len() });
        }
        self.data = SimpleBitStorage::with_data(self.data.get_bits(), 256, data.to_vec()).expect("length checked");
        Ok(())
    }

    pub fn get_raw_data(&self) -> &[i64] {
        self.data.get_raw()
    }

    /// `x + z * 16` for chunk-local `x`/`z` (indexes by `& 15` semantics are the caller's job, as in vanilla).
    fn get_index(x: i32, z: i32) -> usize {
        (x + z * 16) as usize
    }
}
