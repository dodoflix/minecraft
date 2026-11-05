//! `SwampHutStructure`.

use super::swamp_hut_piece::SwampHutPiece;
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::structure::structure::{GenerationContext, GenerationStub, STRUCTURE_TYPES, Structure, StructureSettings, on_top_of_chunk_center};
use linkme::distributed_slice;
use mc_world::heightmap::HeightmapType;
use std::sync::Arc;

pub struct SwampHutStructure {
    settings: StructureSettings,
}

#[distributed_slice(STRUCTURE_TYPES)]
static SWAMP_HUT: TypeEntry<dyn Structure> = TypeEntry {
    name: "swamp_hut",
    decode: |fields, ctx| Ok(Arc::new(SwampHutStructure { settings: StructureSettings::decode(fields, ctx)? })),
};

impl Structure for SwampHutStructure {
    fn settings(&self) -> &StructureSettings {
        &self.settings
    }

    fn find_generation_point<'a>(&'a self, context: &mut GenerationContext<'_>) -> Option<GenerationStub<'a>> {
        on_top_of_chunk_center(context, HeightmapType::WorldSurfaceWg, |builder, context| {
            let chunk_pos = context.chunk_pos;
            builder.add_piece(Box::new(SwampHutPiece::new(&mut context.random, chunk_pos.get_min_block_x(), chunk_pos.get_min_block_z())));
        })
    }

    fn type_name(&self) -> &'static str {
        "swamp_hut"
    }
}
