//! `OceanMonumentStructure`.

use super::ocean_monument_pieces::MonumentBuilding;
use crate::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry};
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::structure::piece::get_random_horizontal_direction;
use crate::world::level::levelgen::structure::pieces::{PiecesContainer, StructurePiecesBuilder};
use crate::world::level::levelgen::structure::structure::{GenerationContext, GenerationStub, STRUCTURE_TYPES, Structure, StructureSettings, on_top_of_chunk_center};
use mc_core::pos::ChunkPos;
use mc_core::random::{LegacyRandomSource, WorldgenRandom, generate_unique_seed};
use mc_world::heightmap::HeightmapType;
use mc_world::palette::BiomeId;
use std::sync::Arc;

pub struct OceanMonumentStructure {
    settings: StructureSettings,
    /// `BiomeTags.REQUIRED_OCEAN_MONUMENT_SURROUNDING`, resolved when the structure is decoded.
    required_surrounding: Vec<BiomeId>,
}

#[linkme::distributed_slice(STRUCTURE_TYPES)]
static OCEAN_MONUMENT: TypeEntry<dyn Structure> = TypeEntry { name: "ocean_monument", decode: OceanMonumentStructure::decode };

impl OceanMonumentStructure {
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Structure>> {
        let settings = StructureSettings::decode(fields, ctx)?;
        let required_surrounding = ctx.biomes.holder_set_from_json(&serde_json::json!("#minecraft:required_ocean_monument_surrounding"))?;
        Ok(Arc::new(OceanMonumentStructure { settings, required_surrounding }))
    }
}

/// `createTopPiece(chunkPos, random)`.
fn create_top_piece(chunk_pos: ChunkPos, random: &mut WorldgenRandom) -> MonumentBuilding {
    let west = chunk_pos.get_min_block_x() - 29;
    let north = chunk_pos.get_min_block_z() - 29;
    let orientation = get_random_horizontal_direction(random);
    MonumentBuilding::new(random, west, north, orientation)
}

/// `generatePieces(builder, context)`.
fn generate_pieces(builder: &mut StructurePiecesBuilder, context: &mut GenerationContext<'_>) {
    let chunk_pos = context.chunk_pos;
    builder.add_piece(Box::new(create_top_piece(chunk_pos, &mut context.random)));
}

impl Structure for OceanMonumentStructure {
    fn settings(&self) -> &StructureSettings {
        &self.settings
    }

    fn find_generation_point<'a>(&'a self, context: &mut GenerationContext<'_>) -> Option<GenerationStub<'a>> {
        let offset_x = context.chunk_pos.get_block_x(9);
        let offset_z = context.chunk_pos.get_block_z(9);
        let sea_level = context.env.get_sea_level();
        for biome in context.biome_resolver.get_biomes_within(offset_x, sea_level, offset_z, 29) {
            if !self.required_surrounding.contains(&biome) {
                return None;
            }
        }
        on_top_of_chunk_center(context, HeightmapType::OceanFloorWg, generate_pieces)
    }

    fn type_name(&self) -> &'static str {
        "ocean_monument"
    }

    /// `regeneratePiecesAfterLoad(chunkPos, seed, savedPieces)`.
    fn regenerate_pieces_after_load(&self, chunk_pos: ChunkPos, seed: i64, saved_pieces: PiecesContainer) -> PiecesContainer {
        if saved_pieces.is_empty() {
            return saved_pieces;
        }
        let mut random = WorldgenRandom::new(Box::new(LegacyRandomSource::new(generate_unique_seed())));
        random.set_large_feature_seed(seed, chunk_pos.x, chunk_pos.z);
        let old_top_piece = &saved_pieces.pieces[0];
        let old_bounding_box = old_top_piece.get_bounding_box();
        let west = old_bounding_box.min_x();
        let north = old_bounding_box.min_z();
        let default_orientation = get_random_horizontal_direction(&mut random);
        let orientation = old_top_piece.get_orientation().unwrap_or(default_orientation);
        let top_piece = MonumentBuilding::new(&mut random, west, north, orientation);
        let mut result = StructurePiecesBuilder::new();
        result.add_piece(Box::new(top_piece));
        result.build()
    }
}
