//! `WoodlandMansionStructure`.

use super::woodland_mansion_pieces;
use crate::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::structure::manager::StructureManager;
use crate::world::level::levelgen::structure::pieces::{PiecesContainer, StructurePiecesBuilder};
use crate::world::level::levelgen::structure::structure::{
    GenerationContext, GenerationStub, STRUCTURE_TYPES, Structure, StructureSettings, get_lowest_y_in_5_by_5_box,
};
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::bounding_box::BoundingBox;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_core::rotation::Rotation;
use mc_data::block::Block;
use std::sync::Arc;

const START_OFFSET_IN_CHUNK: i32 = 7;

pub struct WoodlandMansionStructure {
    settings: StructureSettings,
}

#[linkme::distributed_slice(STRUCTURE_TYPES)]
static WOODLAND_MANSION: TypeEntry<dyn Structure> = TypeEntry { name: "woodland_mansion", decode: WoodlandMansionStructure::decode };

impl WoodlandMansionStructure {
    /// `simpleCodec(WoodlandMansionStructure::new)`.
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Structure>> {
        Ok(Arc::new(WoodlandMansionStructure { settings: StructureSettings::decode(fields, ctx)? }))
    }
}

/// `generatePieces(builder, context, startPos, rotation)`.
fn generate_pieces(builder: &mut StructurePiecesBuilder, context: &mut GenerationContext<'_>, start_pos: BlockPos, rotation: Rotation) {
    let mut wm_pieces = Vec::new();
    woodland_mansion_pieces::generate_mansion(&context.env.structure_template_manager, start_pos, rotation, &mut wm_pieces, &mut context.random);
    for piece in wm_pieces {
        builder.add_piece(Box::new(piece));
    }
}

impl Structure for WoodlandMansionStructure {
    fn settings(&self) -> &StructureSettings {
        &self.settings
    }

    fn type_name(&self) -> &'static str {
        "woodland_mansion"
    }

    fn find_generation_point<'a>(&'a self, context: &mut GenerationContext<'_>) -> Option<GenerationStub<'a>> {
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
        Some(GenerationStub::new(start_pos, move |builder, context| generate_pieces(builder, context, start_pos, rotation)))
    }

    /// `afterPlace(level, structureManager, generator, random, chunkBB, chunkPos, pieces)`: cobblestone pillars below the
    /// mansion's lowest layer.
    fn after_place(
        &self,
        level: &mut dyn WorldGenLevel,
        _structure_manager: &StructureManager,
        _generator: &GenEnv,
        _random: &mut dyn RandomSource,
        chunk_bb: &BoundingBox,
        _chunk_pos: ChunkPos,
        pieces: &PiecesContainer,
    ) {
        let min_y = level.get_min_y();
        let bounding_box = pieces.calculate_bounding_box();
        let y_start = bounding_box.min_y();
        for x in chunk_bb.min_x()..=chunk_bb.max_x() {
            for z in chunk_bb.min_z()..=chunk_bb.max_z() {
                let mut pos = BlockPos::new(x, y_start, z);
                if !level.is_empty_block(pos) && bounding_box.is_inside(&pos) && pieces.is_inside_piece(pos) {
                    let mut y = y_start - 1;
                    while y > min_y {
                        pos = BlockPos::new(x, y, z);
                        if !level.is_empty_block(pos) && !level.get_block_state(pos).liquid() {
                            break;
                        }
                        level.set_block_flags(pos, Block::COBBLESTONE.default_state(), 2);
                        y -= 1;
                    }
                }
            }
        }
    }
}
