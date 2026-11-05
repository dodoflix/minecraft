//! `MineshaftStructure`: the abandoned mineshaft (`mineshaft`, `mineshaft_mesa`): one start room plus the recursive
//! corridor/crossing/stairs tree of [`mineshaft_pieces`](super::mineshaft_pieces), moved below sea level (normal) or to
//! a random height between sea level and the surface (mesa).

use super::mineshaft_pieces::MineShaftRoom;
use mc_game_core::world::level::levelgen::codec::{Fields, LoadError, LoadResult, TypeEntry, parse_str};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::levelgen::structure::pieces::StructurePiecesBuilder;
use mc_game_core::world::level::levelgen::structure::structure::{GenerationContext, GenerationStub, STRUCTURE_TYPES, Structure, StructureSettings};
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_world::heightmap::HeightmapType;
use std::sync::Arc;

/// `MineshaftStructure.Type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MineshaftType {
    Normal,
    Mesa,
}

impl MineshaftType {
    /// `Type.byId(id)` (`ByIdMap.continuous(.., OutOfBoundsStrategy.ZERO)`).
    pub fn by_id(id: i32) -> MineshaftType {
        match id {
            1 => MineshaftType::Mesa,
            _ => MineshaftType::Normal,
        }
    }

    /// `Type.ordinal()`.
    pub fn ordinal(self) -> i32 {
        self as i32
    }

    /// `Type.getSerializedName()`.
    pub fn get_name(self) -> &'static str {
        match self {
            MineshaftType::Normal => "normal",
            MineshaftType::Mesa => "mesa",
        }
    }

    /// `Type.CODEC` (`StringRepresentable.fromEnum`).
    fn from_name(name: &str) -> Option<MineshaftType> {
        [MineshaftType::Normal, MineshaftType::Mesa].into_iter().find(|t| t.get_name() == name)
    }

    /// `Type.getWoodState().getBlock()`.
    pub fn wood(self) -> Block {
        match self {
            MineshaftType::Normal => Block::OAK_LOG,
            MineshaftType::Mesa => Block::DARK_OAK_LOG,
        }
    }

    /// `Type.getPlanksState().getBlock()`.
    pub fn planks(self) -> Block {
        match self {
            MineshaftType::Normal => Block::OAK_PLANKS,
            MineshaftType::Mesa => Block::DARK_OAK_PLANKS,
        }
    }

    /// `Type.getFenceState().getBlock()`.
    pub fn fence(self) -> Block {
        match self {
            MineshaftType::Normal => Block::OAK_FENCE,
            MineshaftType::Mesa => Block::DARK_OAK_FENCE,
        }
    }
}

pub struct MineshaftStructure {
    settings: StructureSettings,
    mineshaft_type: MineshaftType,
}

impl MineshaftStructure {
    pub fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Structure>> {
        let settings = StructureSettings::decode(fields, ctx)?;
        let mineshaft_type = fields.required("mineshaft_type", |v| {
            let name = parse_str(v)?;
            MineshaftType::from_name(name).ok_or_else(|| LoadError::new(format!("Unknown element name:{name}")))
        })?;
        Ok(Arc::new(MineshaftStructure { settings, mineshaft_type }))
    }

    /// `generatePiecesAndAdjust(builder, context)`.
    fn generate_pieces_and_adjust(&self, builder: &mut StructurePiecesBuilder, context: &mut GenerationContext<'_>) -> i32 {
        let chunk_pos = context.chunk_pos;
        let mine_shaft_room = MineShaftRoom::new(0, &mut context.random, chunk_pos.get_block_x(2), chunk_pos.get_block_z(2), self.mineshaft_type);
        builder.add_piece_and_children(Box::new(mine_shaft_room), None, &mut context.random);
        let sea_level = context.env.get_sea_level();
        if self.mineshaft_type == MineshaftType::Mesa {
            let center = builder.get_bounding_box().get_center();
            let surface_height = context.env.get_base_height(center.x(), center.z(), HeightmapType::WorldSurfaceWg, context.height_accessor);
            let target_y_for_center =
                if surface_height <= sea_level { sea_level } else { mth::random_between_inclusive(&mut context.random, sea_level, surface_height) };
            let dy = target_y_for_center.wrapping_sub(center.y());
            builder.offset_pieces_vertically(dy);
            dy
        } else {
            builder.move_below_sea_level(sea_level, context.env.get_min_y(), &mut context.random, 10)
        }
    }
}

impl Structure for MineshaftStructure {
    fn settings(&self) -> &StructureSettings {
        &self.settings
    }

    fn find_generation_point<'a>(&'a self, context: &mut GenerationContext<'_>) -> Option<GenerationStub<'a>> {
        context.random.next_double();
        let chunk_pos = context.chunk_pos;
        let start_pos = BlockPos::new(chunk_pos.get_middle_block_x(), 50, chunk_pos.get_min_block_z());
        let mut builder = StructurePiecesBuilder::new();
        let y_offset = self.generate_pieces_and_adjust(&mut builder, context);
        Some(GenerationStub::with_builder(BlockPos::new(start_pos.x(), start_pos.y().wrapping_add(y_offset), start_pos.z()), builder))
    }

    fn type_name(&self) -> &'static str {
        "mineshaft"
    }
}

#[linkme::distributed_slice(STRUCTURE_TYPES)]
static MINESHAFT: TypeEntry<dyn Structure> = TypeEntry { name: "mineshaft", decode: MineshaftStructure::decode };
