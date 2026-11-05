//! `StructureStart`: a placed structure's pieces plus bookkeeping (origin chunk, reference count), and its NBT form
//! (`structures.starts.<id>` of the origin chunk).

use super::piece::StructurePieceSerializationContext;
use super::pieces::PiecesContainer;
use super::structure::Structure;
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::holder::Holder;
use mc_core::bounding_box::BoundingBox;
use mc_core::ident::Identifier;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_nbt::{CompoundTag, Tag};
use super::manager::StructureManager;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use std::sync::{Arc, Mutex, MutexGuard};

/// `StructureStart.INVALID_START_ID`.
pub const INVALID_START_ID: &str = "INVALID";

/// `StructureStart`.
pub struct StructureStart {
    /// `None` for `INVALID_START`.
    structure: Option<Holder<dyn Structure>>,
    pub piece_container: PiecesContainer,
    chunk_pos: ChunkPos,
    references: i32,
    cached_bounding_box: Option<BoundingBox>,
}

impl StructureStart {
    /// `new StructureStart(structure, chunkPos, references, pieceContainer)`.
    pub fn new(structure: Holder<dyn Structure>, chunk_pos: ChunkPos, references: i32, piece_container: PiecesContainer) -> StructureStart {
        StructureStart { structure: Some(structure), piece_container, chunk_pos, references, cached_bounding_box: None }
    }

    /// `INVALID_START`.
    pub fn invalid() -> StructureStart {
        StructureStart { structure: None, piece_container: PiecesContainer::new(Vec::new()), chunk_pos: ChunkPos::new(0, 0), references: 0, cached_bounding_box: None }
    }

    /// `loadStaticStart(context, tag, seed)`: `Ok(INVALID)` for `id: INVALID`, `Err` for an unknown structure id (vanilla
    /// logs and returns `null`).
    pub fn load_static_start(context: &StructurePieceSerializationContext<'_>, tag: &CompoundTag, seed: i64) -> Result<StructureStart, String> {
        let id = tag.get_string_or("id", "");
        if id == INVALID_START_ID {
            return Ok(StructureStart::invalid());
        }
        let key = Identifier::try_parse(id).ok_or_else(|| format!("Unknown stucture id: {id}"))?;
        let structure = context.env.structures.structures.get(&key).ok_or_else(|| format!("Unknown stucture id: {id}"))?.clone();
        let chunk_pos = ChunkPos::new(tag.get_int_or("ChunkX", 0), tag.get_int_or("ChunkZ", 0));
        let references = tag.get_int_or("references", 0);
        let children = tag.get_list_or_empty("Children");
        let mut pieces = PiecesContainer::load(children, context);
        pieces = structure.value().regenerate_pieces_after_load(chunk_pos, seed, pieces);
        Ok(StructureStart::new(structure, chunk_pos, references, pieces))
    }

    /// `getBoundingBox()` (cached on first use, like vanilla's `cachedBoundingBox`).
    pub fn get_bounding_box(&mut self) -> BoundingBox {
        if let Some(bb) = self.cached_bounding_box {
            return bb;
        }
        let structure = self.structure.as_ref().expect("INVALID_START has no bounding box");
        let bb = structure.value().adjust_bounding_box(self.piece_container.calculate_bounding_box());
        self.cached_bounding_box = Some(bb);
        bb
    }

    /// `placeInChunk(level, structureManager, generator, random, chunkBB, chunkPos)`.
    pub fn place_in_chunk(
        &mut self,
        level: &mut dyn WorldGenLevel,
        structure_manager: &StructureManager<'_>,
        generator: &GenEnv,
        random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
        chunk_pos: ChunkPos,
    ) {
        if self.piece_container.pieces.is_empty() {
            return;
        }
        let center_bb = self.piece_container.pieces[0].base().bounding_box;
        let center_pos = center_bb.get_center();
        let reference_pos = BlockPos::new(center_pos.x(), center_bb.min_y(), center_pos.z());
        for next in &mut self.piece_container.pieces {
            if next.base().bounding_box.intersects(chunk_bb) {
                next.post_process(level, structure_manager, generator, random, chunk_bb, chunk_pos, reference_pos);
            }
        }
        if let Some(structure) = &self.structure {
            structure.value().after_place(level, structure_manager, generator, random, chunk_bb, chunk_pos, &self.piece_container);
        }
    }

    /// `createTag(context, chunkPos)`.
    pub fn create_tag(&self, context: &StructurePieceSerializationContext<'_>, chunk_pos: ChunkPos) -> CompoundTag {
        let mut tag = CompoundTag::new();
        if self.is_valid() {
            let id = self.structure.as_ref().and_then(Holder::id).expect("registered structure");
            tag.put_string("id", id.to_string());
            tag.put_int("ChunkX", chunk_pos.x);
            tag.put_int("ChunkZ", chunk_pos.z);
            tag.put_int("references", self.references);
            tag.put("Children", Tag::List(self.piece_container.save(context)));
        } else {
            tag.put_string("id", INVALID_START_ID);
        }
        tag
    }

    pub fn is_valid(&self) -> bool {
        !self.piece_container.is_empty()
    }

    pub fn get_chunk_pos(&self) -> ChunkPos {
        self.chunk_pos
    }

    /// `canBeReferenced()` (`getMaxReferences()` is 1).
    pub fn can_be_referenced(&self) -> bool {
        self.references < 1
    }

    /// `addReference()`.
    pub fn add_reference(&mut self) {
        self.references += 1;
    }

    pub fn get_references(&self) -> i32 {
        self.references
    }

    /// `getStructure()`.
    pub fn get_structure(&self) -> &Holder<dyn Structure> {
        self.structure.as_ref().expect("INVALID_START has no structure")
    }

    pub fn structure_id(&self) -> Option<&Identifier> {
        self.structure.as_ref().and_then(Holder::id)
    }
}

/// A parsed start shared between the starts cache and the placing threads (pieces are mutated while placing, e.g.
/// `ScatteredFeaturePiece` moves its box on first placement, exactly as vanilla mutates the chunk's start object).
#[derive(Clone)]
pub struct StartHandle(pub Arc<Mutex<StructureStart>>);

impl StartHandle {
    pub fn new(start: StructureStart) -> StartHandle {
        StartHandle(Arc::new(Mutex::new(start)))
    }

    pub fn lock(&self) -> MutexGuard<'_, StructureStart> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn ptr_eq(&self, other: &StartHandle) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
