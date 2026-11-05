//! `NetherFossilStructure`.

use super::nether_fossil_pieces;
use mc_game_core::world::block_entity::BlockEntity;
use mc_game_core::world::level::block::BlockStateExt as _;
use mc_game_core::world::level::levelgen::codec::*;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::levelgen::structure::structure::{GenerationContext, GenerationStub, STRUCTURE_TYPES, Structure, StructureSettings};
use mc_game_core::world::level::{BlockGetter, LevelHeight};
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_data::fluid::FluidState;
use mc_worldgen::generator::BlockColumn;
use mc_worldgen::provider::HeightProvider;
use std::sync::Arc;

/// `EmptyBlockGetter.INSTANCE`.
struct EmptyBlockGetter;

impl BlockGetter for EmptyBlockGetter {
    fn get_block_state(&self, _pos: BlockPos) -> BlockState {
        Block::AIR.default_state()
    }

    fn get_fluid_state(&self, _pos: BlockPos) -> FluidState {
        mc_data::fluid::Fluid::EMPTY.default_state()
    }

    fn get_block_entity(&self, _pos: BlockPos) -> Option<&dyn BlockEntity> {
        None
    }

    fn level_height(&self) -> LevelHeight {
        LevelHeight::new(0, 0)
    }
}

pub struct NetherFossilStructure {
    pub settings: StructureSettings,
    pub height: HeightProvider,
}

impl NetherFossilStructure {
    /// `NetherFossilStructure.CODEC`.
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Structure>> {
        Ok(Arc::new(NetherFossilStructure { settings: StructureSettings::decode(fields, ctx)?, height: fields.required("height", HeightProvider::from_json)? }))
    }
}

impl Structure for NetherFossilStructure {
    fn settings(&self) -> &StructureSettings {
        &self.settings
    }

    fn type_name(&self) -> &'static str {
        "nether_fossil"
    }

    fn find_generation_point<'a>(&'a self, context: &mut GenerationContext<'_>) -> Option<GenerationStub<'a>> {
        let block_x = context.chunk_pos.get_min_block_x() + context.random.next_int_bounded(16);
        let block_z = context.chunk_pos.get_min_block_z() + context.random.next_int_bounded(16);
        let sea_level = context.env.get_sea_level();
        let generation_context = context.env.world_generation_context(context.height_accessor);
        let mut y = self.height.sample(&mut context.random, &generation_context);
        let column = context.get_base_column(block_x, block_z);
        while y > sea_level {
            let current = column.get_block(y);
            y -= 1;
            let below = column.get_block(y);
            if current.is_air() && (below.block() == Block::SOUL_SAND || below.is_face_sturdy_at(&EmptyBlockGetter, BlockPos::new(block_x, y, block_z), Direction::Up)) {
                break;
            }
        }
        if y <= sea_level {
            return None;
        }
        let position = BlockPos::new(block_x, y, block_z);
        Some(GenerationStub::new(position, move |builder, context| {
            nether_fossil_pieces::add_pieces(&context.env.structure_template_manager, builder, &mut context.random, position);
        }))
    }
}

#[linkme::distributed_slice(STRUCTURE_TYPES)]
static NETHER_FOSSIL: TypeEntry<dyn Structure> = TypeEntry { name: "nether_fossil", decode: NetherFossilStructure::decode };
