//! `IglooStructure`.

use super::igloo_pieces;
use crate::world::level::levelgen::codec::TypeEntry;
use crate::world::level::levelgen::structure::pieces::StructurePiecesBuilder;
use crate::world::level::levelgen::structure::structure::{
    GenerationContext, GenerationStub, STRUCTURE_TYPES, Structure, StructureSettings, on_top_of_chunk_center,
};
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_core::rotation::Rotation;
use mc_world::heightmap::HeightmapType;
use std::sync::Arc;

pub struct IglooStructure {
    settings: StructureSettings,
}

#[distributed_slice(STRUCTURE_TYPES)]
static IGLOO: TypeEntry<dyn Structure> = TypeEntry {
    name: "igloo",
    decode: |fields, ctx| Ok(Arc::new(IglooStructure { settings: StructureSettings::decode(fields, ctx)? })),
};

impl IglooStructure {
    /// `generatePieces(builder, context)`.
    fn generate_pieces(builder: &mut StructurePiecesBuilder, context: &mut GenerationContext<'_>) {
        let chunk_pos = context.chunk_pos;
        let start_pos = BlockPos::new(chunk_pos.get_min_block_x(), 90, chunk_pos.get_min_block_z());
        let rotation = Rotation::get_random(&mut context.random);
        igloo_pieces::add_pieces(&context.env.structure_template_manager, start_pos, rotation, builder, &mut context.random);
    }
}

impl Structure for IglooStructure {
    fn settings(&self) -> &StructureSettings {
        &self.settings
    }

    fn find_generation_point<'a>(&'a self, context: &mut GenerationContext<'_>) -> Option<GenerationStub<'a>> {
        on_top_of_chunk_center(context, HeightmapType::WorldSurfaceWg, Self::generate_pieces)
    }

    fn type_name(&self) -> &'static str {
        "igloo"
    }
}
