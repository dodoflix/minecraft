//! `WorldGenContext`, the chunk generator seam and `WorldGenRegion` (the chunk neighbourhood a generation step works on).
//!
//! The state-only generation stages (biomes, terrain) are `mc_worldgen`'s `ChunkGenerator`; the stages that need block
//! behaviours (structure starts/references, feature decoration, mob spawning) are [`WorldGenHooks`], implemented by the
//! structures/features slices (`levelgen::hooks::VanillaWorldGenHooks`), one per level (`ServerLevelParams::hooks`).

use crate::world::level::DimensionKey;
use mc_core::ident::Identifier;
use mc_core::pos::ChunkPos;
use mc_world::chunk::{LevelHeight, ProtoChunk, StructureData};
use mc_world::palette::PalettedContainerFactory;
use std::collections::HashMap;
use std::sync::Arc;

/// A chunk of a [`WorldGenRegion`]: lent (writable) or a read-only snapshot of a chunk the game thread owns
/// (vanilla's `ImposterProtoChunk(allowWrites = false)` over a full chunk).
pub enum GenChunk {
    Loaned(Box<ProtoChunk>),
    Snapshot(Box<ProtoChunk>),
}

impl GenChunk {
    pub fn proto(&self) -> &ProtoChunk {
        match self {
            GenChunk::Loaned(c) | GenChunk::Snapshot(c) => c,
        }
    }

    pub fn proto_mut(&mut self) -> &mut ProtoChunk {
        match self {
            GenChunk::Loaned(c) | GenChunk::Snapshot(c) => c,
        }
    }

    pub fn is_writable(&self) -> bool {
        matches!(self, GenChunk::Loaned(_))
    }
}

/// The neighbourhood of a generation step.
pub struct WorldGenRegion {
    pub center: ChunkPos,
    /// `ChunkStep.directDependencies` radius: chunks farther away are not accessible.
    pub read_radius: i32,
    /// `ChunkStep.blockStateWriteRadius`.
    pub write_radius: i32,
    chunks: HashMap<i64, GenChunk>,
    /// Structure data of chunks outside the lent set (structure starts are read up to 8 chunks away).
    structure_views: HashMap<i64, Arc<StructureData>>,
    pub level_height: LevelHeight,
}

impl WorldGenRegion {
    pub fn new(
        center: ChunkPos,
        read_radius: i32,
        write_radius: i32,
        chunks: HashMap<i64, GenChunk>,
        structure_views: HashMap<i64, Arc<StructureData>>,
        level_height: LevelHeight,
    ) -> Self {
        Self { center, read_radius, write_radius, chunks, structure_views, level_height }
    }

    /// `getCenter()`.
    pub fn get_center(&self) -> ChunkPos {
        self.center
    }

    /// `hasChunk(x, z)`: inside the step's read radius.
    pub fn has_chunk(&self, x: i32, z: i32) -> bool {
        self.center.get_chessboard_distance_xz(x, z) <= self.read_radius && self.chunks.contains_key(&ChunkPos::pack_xz(x, z))
    }

    /// The region's entry for the chunk (lent or read-only snapshot), without any radius check.
    pub fn gen_chunk(&self, x: i32, z: i32) -> Option<&GenChunk> {
        self.chunks.get(&ChunkPos::pack_xz(x, z))
    }

    pub fn get_chunk(&self, x: i32, z: i32) -> Option<&ProtoChunk> {
        self.chunks.get(&ChunkPos::pack_xz(x, z)).map(GenChunk::proto)
    }

    /// Writable access: only inside the block state write radius (`ensureCanWrite`).
    pub fn get_chunk_mut(&mut self, x: i32, z: i32) -> Option<&mut ProtoChunk> {
        // The center chunk is always writable (steps with write radius -1 still fill their own chunk).
        let within = self.center.get_chessboard_distance_xz(x, z) <= self.write_radius.max(0);
        if !within {
            return None;
        }
        match self.chunks.get_mut(&ChunkPos::pack_xz(x, z)) {
            Some(c) if c.is_writable() => Some(c.proto_mut()),
            _ => None,
        }
    }

    /// The center chunk of the step (always lent).
    pub fn get_chunk_mut_unchecked(&mut self, pos: ChunkPos) -> &mut ProtoChunk {
        self.chunks.get_mut(&pos.pack()).expect("chunk is part of the region").proto_mut()
    }

    /// Structure starts/references of any chunk within reach.
    pub fn structure_data(&self, pos: ChunkPos) -> Option<&StructureData> {
        let key = pos.pack();
        match self.chunks.get(&key) {
            Some(c) => Some(&c.proto().access.structures),
            None => self.structure_views.get(&key).map(|a| &**a),
        }
    }

    /// Take a chunk out so a generator can mutate it while reading the rest of the region.
    pub fn take_chunk(&mut self, pos: ChunkPos) -> Option<GenChunk> {
        self.chunks.remove(&pos.pack())
    }

    pub fn put_chunk(&mut self, pos: ChunkPos, chunk: GenChunk) {
        self.chunks.insert(pos.pack(), chunk);
    }

    pub fn into_chunks(self) -> HashMap<i64, GenChunk> {
        self.chunks
    }
}

/// The state-only chunk generator stages (adapter over `mc_worldgen`'s `ChunkGenerator`).
pub trait ChunkGeneratorBackend: Send + Sync {
    /// `getTypeNameForDataFixer()`.
    fn type_name_for_data_fixer(&self) -> Option<Identifier>;
    /// `createBiomes(randomState, blender, structureManager, chunk)`.
    fn create_biomes(&self, chunk: &mut ProtoChunk, region: &WorldGenRegion);
    /// `buildTerrain(chunk, blender, randomState, structureManager, biomeManager, region, possibleBiomes)`:
    /// noise fill, surface and carvers of the one chunk.
    fn build_terrain(&self, chunk: &mut ProtoChunk, region: &WorldGenRegion);
    /// `getOrigin(randomState)`.
    fn get_origin(&self) -> ChunkPos {
        ChunkPos::ZERO
    }
    /// `getSpawnHeight(heightAccessor)`.
    fn get_spawn_height(&self, _height: LevelHeight) -> i32 {
        64
    }
    /// `getBaseHeight(x, z, type, heightAccessor, randomState)`.
    fn get_base_height(&self, x: i32, z: i32, ty: mc_world::heightmap::HeightmapType, height: LevelHeight) -> i32;
}

/// The generation stages that need block behaviours (structures, features, mob spawning). Implemented by the
/// structures/features slices.
pub trait WorldGenHooks: Send + Sync {
    /// `createStructures`: the structure starts of `chunk` (only called when `generate_structures` is on).
    fn create_structures(&self, ctx: &WorldGenContext, chunk: &mut ProtoChunk);
    /// `createReferences(region, structureManager, chunk)`.
    fn create_references(&self, ctx: &WorldGenContext, region: &mut WorldGenRegion);
    /// `applyBiomeDecoration(region, chunk, structureManager)`.
    fn apply_biome_decoration(&self, ctx: &WorldGenContext, region: &mut WorldGenRegion);
    /// `spawnOriginalMobs(region)`.
    fn spawn_original_mobs(&self, ctx: &WorldGenContext, region: &mut WorldGenRegion);
    /// `level.onStructureStartsAvailable(chunk)` (structure check cache).
    fn on_structure_starts_available(&self, _chunk: &ProtoChunk) {}
}

/// The hooks of a level without structure/feature/mob generation (terrain only).
pub struct NoWorldGenHooks;

impl WorldGenHooks for NoWorldGenHooks {
    fn create_structures(&self, _ctx: &WorldGenContext, _chunk: &mut ProtoChunk) {}
    fn create_references(&self, _ctx: &WorldGenContext, _region: &mut WorldGenRegion) {}
    fn apply_biome_decoration(&self, _ctx: &WorldGenContext, _region: &mut WorldGenRegion) {}
    fn spawn_original_mobs(&self, _ctx: &WorldGenContext, _region: &mut WorldGenRegion) {}
}

/// `WorldGenContext` (the parts the worker threads need; everything is shared and immutable).
pub struct WorldGenContext {
    pub dimension: DimensionKey,
    pub level_height: LevelHeight,
    pub factory: Arc<PalettedContainerFactory>,
    pub generator: Arc<dyn ChunkGeneratorBackend>,
    pub hooks: Arc<dyn WorldGenHooks>,
    /// `getWorldGenSettings().options().generateStructures()`.
    pub generate_structures: bool,
    pub seed: i64,
}


