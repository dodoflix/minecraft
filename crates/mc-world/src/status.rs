//! `net.minecraft.world.level.chunk.status.{ChunkStatus, ChunkType}`: the static data of the ten chunk statuses
//! (the generation pipeline `ChunkPyramid`/`ChunkStep` is not part of this module).

use mc_core::ident::Identifier;
use mc_nbt::Tag;

use crate::heightmap::HeightmapType;

/// `ChunkType`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ChunkType {
    ProtoChunk,
    LevelChunk,
}

/// `ChunkStatus` in registry order (`minecraft:chunk_status` protocol ids 0..=9, equal to `getIndex()`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum ChunkStatus {
    Empty,
    StructureStarts,
    StructureReferences,
    Biomes,
    Terrain,
    Features,
    InitializeLight,
    Light,
    Spawn,
    Full,
}

/// `ChunkStatus.MAX_STRUCTURE_DISTANCE`.
pub const MAX_STRUCTURE_DISTANCE: i32 = 8;

/// `ChunkStatus.WORLDGEN_HEIGHTMAPS` (`EnumSet` ordinal order).
pub const WORLDGEN_HEIGHTMAPS: &[HeightmapType] =
    &[HeightmapType::WorldSurfaceWg, HeightmapType::OceanFloorWg];

/// `ChunkStatus.FINAL_HEIGHTMAPS` (`EnumSet` ordinal order).
pub const FINAL_HEIGHTMAPS: &[HeightmapType] = &[
    HeightmapType::WorldSurface,
    HeightmapType::OceanFloor,
    HeightmapType::MotionBlocking,
    HeightmapType::MotionBlockingNoLeaves,
];

impl ChunkStatus {
    /// Registration order; also `getStatusList()` (`EMPTY` .. `FULL`).
    pub const ALL: [ChunkStatus; 10] = [
        ChunkStatus::Empty,
        ChunkStatus::StructureStarts,
        ChunkStatus::StructureReferences,
        ChunkStatus::Biomes,
        ChunkStatus::Terrain,
        ChunkStatus::Features,
        ChunkStatus::InitializeLight,
        ChunkStatus::Light,
        ChunkStatus::Spawn,
        ChunkStatus::Full,
    ];

    /// `getStatusList()`.
    pub fn get_status_list() -> &'static [ChunkStatus] {
        &Self::ALL
    }

    /// `getIndex()`, which is also the registry protocol id.
    pub const fn index(self) -> i32 {
        self as i32
    }

    /// `BuiltInRegistries.CHUNK_STATUS.getId(status)`.
    pub const fn id(self) -> u32 {
        self as u32
    }

    /// Registry lookup by protocol id (`None` for out-of-range ids).
    pub fn from_id(id: u32) -> Option<ChunkStatus> {
        Self::ALL.get(id as usize).copied()
    }

    /// `getParent()` (`EMPTY` is its own parent).
    pub fn parent(self) -> ChunkStatus {
        match self {
            ChunkStatus::Empty => ChunkStatus::Empty,
            _ => Self::ALL[self as usize - 1],
        }
    }

    pub const fn chunk_type(self) -> ChunkType {
        match self {
            ChunkStatus::Full => ChunkType::LevelChunk,
            _ => ChunkType::ProtoChunk,
        }
    }

    /// `heightmapsAfter()`.
    pub const fn heightmaps_after(self) -> &'static [HeightmapType] {
        match self {
            ChunkStatus::Empty
            | ChunkStatus::StructureStarts
            | ChunkStatus::StructureReferences
            | ChunkStatus::Biomes => WORLDGEN_HEIGHTMAPS,
            _ => FINAL_HEIGHTMAPS,
        }
    }

    pub fn is_or_after(self, step: ChunkStatus) -> bool {
        self >= step
    }

    pub fn is_after(self, step: ChunkStatus) -> bool {
        self > step
    }

    pub fn is_or_before(self, step: ChunkStatus) -> bool {
        self <= step
    }

    pub fn is_before(self, step: ChunkStatus) -> bool {
        self < step
    }

    /// `ChunkStatus.max(a, b)` (`b` on ties).
    pub fn max(a: ChunkStatus, b: ChunkStatus) -> ChunkStatus {
        if a.is_after(b) { a } else { b }
    }

    /// Registry path without namespace (`"structure_starts"`).
    pub const fn path(self) -> &'static str {
        match self {
            ChunkStatus::Empty => "empty",
            ChunkStatus::StructureStarts => "structure_starts",
            ChunkStatus::StructureReferences => "structure_references",
            ChunkStatus::Biomes => "biomes",
            ChunkStatus::Terrain => "terrain",
            ChunkStatus::Features => "features",
            ChunkStatus::InitializeLight => "initialize_light",
            ChunkStatus::Light => "light",
            ChunkStatus::Spawn => "spawn",
            ChunkStatus::Full => "full",
        }
    }

    /// `getName()` / `toString()`: the registry key including namespace (`"minecraft:full"`), as stored in the `Status` tag.
    pub fn name(self) -> String {
        format!("minecraft:{}", self.path())
    }

    /// `ChunkStatus.CODEC` on a string: `Identifier.CODEC` then registry lookup. `None` for unparsable
    /// identifiers and unknown keys.
    pub fn decode_str(key: &str) -> Option<ChunkStatus> {
        let id = Identifier::read(key).ok()?;
        if id.namespace() != "minecraft" {
            return None;
        }
        Self::ALL.into_iter().find(|s| s.path() == id.path())
    }

    /// `ChunkStatus.CODEC` on an NBT tag (only string tags decode). Callers like the chunk reader use
    /// `.unwrap_or(ChunkStatus::Empty)`.
    pub fn decode(tag: &Tag) -> Option<ChunkStatus> {
        Self::decode_str(tag.as_string()?)
    }

    /// `ChunkStatus.byName(key)`: unparsable or unknown keys give the registry default `EMPTY`.
    pub fn by_name(key: &str) -> ChunkStatus {
        Self::decode_str(key).unwrap_or(ChunkStatus::Empty)
    }
}

impl std::fmt::Display for ChunkStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "minecraft:{}", self.path())
    }
}
