//! `EndCityStructure`.

use super::end_city_pieces;
use mc_core::pos::BlockPos;
use mc_core::rotation::Rotation;
use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::levelgen::structure::structure::{
    GenerationContext, GenerationStub, STRUCTURE_TYPES, Structure, StructureSettings,
    get_lowest_y_in_5_by_5_box,
};
use std::sync::Arc;

const START_OFFSET_IN_CHUNK: i32 = 7;

pub struct EndCityStructure {
    settings: StructureSettings,
}

#[linkme::distributed_slice(STRUCTURE_TYPES)]
static END_CITY: TypeEntry<dyn Structure> = TypeEntry {
    name: "end_city",
    decode: EndCityStructure::decode,
};

impl EndCityStructure {
    /// `simpleCodec(EndCityStructure::new)`.
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Structure>> {
        Ok(Arc::new(EndCityStructure {
            settings: StructureSettings::decode(fields, ctx)?,
        }))
    }
}

impl Structure for EndCityStructure {
    fn settings(&self) -> &StructureSettings {
        &self.settings
    }

    fn type_name(&self) -> &'static str {
        "end_city"
    }

    fn find_generation_point<'a>(
        &'a self,
        context: &mut GenerationContext<'_>,
    ) -> Option<GenerationStub<'a>> {
        let block_x = context.chunk_pos.get_block_x(START_OFFSET_IN_CHUNK);
        let block_z = context.chunk_pos.get_block_z(START_OFFSET_IN_CHUNK);
        if !context.could_valid_biome_exist_in_terrain_column(block_x, block_z) {
            return None;
        }
        let rotation = Rotation::get_random(&mut context.random);
        let start_pos = get_lowest_y_in_5_by_5_box(context, block_x, block_z, rotation);
        if start_pos.y() < 60 {
            return None;
        }
        Some(GenerationStub::new(start_pos, move |builder, context| {
            generate_pieces(builder, start_pos, rotation, context)
        }))
    }
}

/// `generatePieces(builder, startPos, rotation, context)`.
fn generate_pieces(
    builder: &mut mc_game_core::world::level::levelgen::structure::pieces::StructurePiecesBuilder,
    start_pos: BlockPos,
    rotation: Rotation,
    context: &mut GenerationContext<'_>,
) {
    let mut pieces = Vec::new();
    end_city_pieces::start_house_tower(
        &context.env.structure_template_manager,
        start_pos,
        rotation,
        &mut pieces,
        &mut context.random,
    );
    for piece in pieces {
        builder.add_piece(Box::new(piece));
    }
}
