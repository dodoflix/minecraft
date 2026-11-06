//! `BuriedTreasureStructure`.

use super::buried_treasure_pieces::BuriedTreasurePiece;
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::structure::structure::{
    GenerationContext, GenerationStub, STRUCTURE_TYPES, Structure, StructureSettings,
    on_top_of_chunk_center,
};
use mc_world::heightmap::HeightmapType;
use std::sync::Arc;

pub struct BuriedTreasureStructure {
    settings: StructureSettings,
}

#[distributed_slice(STRUCTURE_TYPES)]
static BURIED_TREASURE: TypeEntry<dyn Structure> = TypeEntry {
    name: "buried_treasure",
    decode: |fields, ctx| {
        Ok(Arc::new(BuriedTreasureStructure {
            settings: StructureSettings::decode(fields, ctx)?,
        }))
    },
};

impl Structure for BuriedTreasureStructure {
    fn settings(&self) -> &StructureSettings {
        &self.settings
    }

    fn find_generation_point<'a>(
        &'a self,
        context: &mut GenerationContext<'_>,
    ) -> Option<GenerationStub<'a>> {
        on_top_of_chunk_center(context, HeightmapType::OceanFloorWg, |builder, context| {
            let offset = BlockPos::new(
                context.chunk_pos.get_block_x(9),
                90,
                context.chunk_pos.get_block_z(9),
            );
            builder.add_piece(Box::new(BuriedTreasurePiece::new(offset)));
        })
    }

    fn type_name(&self) -> &'static str {
        "buried_treasure"
    }
}
