//! `SinglePieceStructure`: the shared generation of the structures that are one piece on top of the chunk centre
//! (desert pyramid, jungle temple, swamp hut). A concrete structure holds one and delegates `find_generation_point`.

use mc_game_core::world::level::levelgen::structure::piece::StructurePiece;
use mc_game_core::world::level::levelgen::structure::structure::{GenerationContext, GenerationStub, StructureSettings, get_lowest_y, on_top_of_chunk_center_without_biome_check};
use mc_core::random::WorldgenRandom;
use mc_world::heightmap::HeightmapType;

/// `SinglePieceStructure.PieceConstructor`.
pub type PieceConstructor = fn(&mut WorldgenRandom, i32, i32) -> Box<dyn StructurePiece>;

pub struct SinglePieceStructure {
    pub settings: StructureSettings,
    constructor: PieceConstructor,
    width: i32,
    depth: i32,
}

impl SinglePieceStructure {
    pub fn new(constructor: PieceConstructor, width: i32, depth: i32, settings: StructureSettings) -> SinglePieceStructure {
        SinglePieceStructure { settings, constructor, width, depth }
    }

    /// `findGenerationPoint(context)`.
    pub fn find_generation_point(&self, context: &GenerationContext<'_>) -> Option<GenerationStub<'static>> {
        if !context.could_valid_biome_exist_on_top_of_chunk_center() {
            return None;
        }
        if get_lowest_y(context, self.width, self.depth) < context.env.get_sea_level() {
            return None;
        }
        let constructor = self.constructor;
        on_top_of_chunk_center_without_biome_check(context, HeightmapType::WorldSurfaceWg, move |builder, context| {
            let chunk_pos = context.chunk_pos;
            builder.add_piece(constructor(&mut context.random, chunk_pos.get_min_block_x(), chunk_pos.get_min_block_z()));
        })
    }
}
