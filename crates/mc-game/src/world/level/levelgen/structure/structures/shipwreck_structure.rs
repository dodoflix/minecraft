//! `ShipwreckStructure`.

use super::shipwreck_pieces::{ShipwreckPiece, add_random_piece};
use crate::world::level::levelgen::codec::*;
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::structure::pieces::StructurePiecesBuilder;
use crate::world::level::levelgen::structure::structure::{
    GenerationContext, GenerationStub, STRUCTURE_TYPES, Structure, StructureSettings, get_lowest_y_at, get_mean_first_occupied_height, on_top_of_chunk_center,
};
use mc_core::pos::BlockPos;
use mc_core::rotation::Rotation;
use mc_world::heightmap::HeightmapType;
use std::sync::Arc;

pub struct ShipwreckStructure {
    pub settings: StructureSettings,
    pub is_beached: bool,
}

impl ShipwreckStructure {
    /// `ShipwreckStructure.CODEC`.
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Structure>> {
        Ok(Arc::new(ShipwreckStructure { settings: StructureSettings::decode(fields, ctx)?, is_beached: fields.required("is_beached", parse_bool)? }))
    }

    fn generate_pieces(&self, builder: &mut StructurePiecesBuilder, context: &mut GenerationContext<'_>) {
        let rotation = Rotation::get_random(&mut context.random);
        let offset = BlockPos::new(context.chunk_pos.get_min_block_x(), 90, context.chunk_pos.get_min_block_z());
        let index = add_random_piece(&context.env.structure_template_manager, offset, rotation, builder, &mut context.random, self.is_beached);
        let piece = builder.get_mut(index).downcast_mut::<ShipwreckPiece>().expect("shipwreck piece");
        if piece.is_too_big_to_fit_in_world_gen_region() {
            let bb = piece.get_bounding_box();
            let height = if self.is_beached {
                // vanilla passes (minX, xSpan, minZ, zSpan) to the (minX, minZ, sizeX, sizeZ) overload
                let min_y = get_lowest_y_at(context, bb.min_x(), bb.get_x_span(), bb.min_z(), bb.get_z_span());
                piece.calculate_beached_position(min_y, &mut context.random)
            } else {
                get_mean_first_occupied_height(context, bb.min_x(), bb.get_x_span(), bb.min_z(), bb.get_z_span())
            };
            piece.adjust_position_height(height);
        }
    }
}

impl Structure for ShipwreckStructure {
    fn settings(&self) -> &StructureSettings {
        &self.settings
    }

    fn type_name(&self) -> &'static str {
        "shipwreck"
    }

    fn find_generation_point<'a>(&'a self, context: &mut GenerationContext<'_>) -> Option<GenerationStub<'a>> {
        let heightmap = if self.is_beached { HeightmapType::WorldSurfaceWg } else { HeightmapType::OceanFloorWg };
        on_top_of_chunk_center(context, heightmap, move |builder, context| self.generate_pieces(builder, context))
    }
}

#[linkme::distributed_slice(STRUCTURE_TYPES)]
static SHIPWRECK: TypeEntry<dyn Structure> = TypeEntry { name: "shipwreck", decode: ShipwreckStructure::decode };
