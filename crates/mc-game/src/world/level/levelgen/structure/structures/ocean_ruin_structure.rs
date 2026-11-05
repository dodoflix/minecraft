//! `OceanRuinStructure`.

use super::ocean_ruin_pieces;
use crate::world::level::levelgen::codec::*;
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::structure::pieces::StructurePiecesBuilder;
use crate::world::level::levelgen::structure::structure::{GenerationContext, GenerationStub, STRUCTURE_TYPES, Structure, StructureSettings, on_top_of_chunk_center};
use mc_core::pos::BlockPos;
use mc_core::rotation::Rotation;
use mc_world::heightmap::HeightmapType;
use std::sync::Arc;

/// `OceanRuinStructure.Type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OceanRuinType {
    Warm,
    Cold,
}

impl OceanRuinType {
    /// `getSerializedName()` (`Type.CODEC`).
    pub fn get_serialized_name(self) -> &'static str {
        match self {
            OceanRuinType::Warm => "warm",
            OceanRuinType::Cold => "cold",
        }
    }

    /// `Type.LEGACY_CODEC` encode (`Enum.toString()`).
    pub fn legacy_name(self) -> &'static str {
        match self {
            OceanRuinType::Warm => "WARM",
            OceanRuinType::Cold => "COLD",
        }
    }

    /// `Type.LEGACY_CODEC` decode (`Type.valueOf`).
    pub fn from_legacy_name(name: &str) -> Option<OceanRuinType> {
        [OceanRuinType::Warm, OceanRuinType::Cold].into_iter().find(|t| t.legacy_name() == name)
    }

    fn from_serialized_name(name: &str) -> Option<OceanRuinType> {
        [OceanRuinType::Warm, OceanRuinType::Cold].into_iter().find(|t| t.get_serialized_name() == name)
    }
}

pub struct OceanRuinStructure {
    pub settings: StructureSettings,
    pub biome_temp: OceanRuinType,
    pub large_probability: f32,
    pub cluster_probability: f32,
}

impl OceanRuinStructure {
    /// `OceanRuinStructure.CODEC`.
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Structure>> {
        Ok(Arc::new(OceanRuinStructure {
            settings: StructureSettings::decode(fields, ctx)?,
            biome_temp: fields.required("biome_temp", |v| {
                let name = parse_str(v)?;
                OceanRuinType::from_serialized_name(name).ok_or_else(|| LoadError::new(format!("Unknown element name:{name}")))
            })?,
            large_probability: fields.required("large_probability", |v| parse_float_range(v, 0.0, 1.0))?,
            cluster_probability: fields.required("cluster_probability", |v| parse_float_range(v, 0.0, 1.0))?,
        }))
    }

    fn generate_pieces(&self, builder: &mut StructurePiecesBuilder, context: &mut GenerationContext<'_>) {
        let offset = BlockPos::new(context.chunk_pos.get_min_block_x(), 90, context.chunk_pos.get_min_block_z());
        let rotation = Rotation::get_random(&mut context.random);
        ocean_ruin_pieces::add_pieces(&context.env.structure_template_manager, offset, rotation, builder, &mut context.random, self);
    }
}

impl Structure for OceanRuinStructure {
    fn settings(&self) -> &StructureSettings {
        &self.settings
    }

    fn type_name(&self) -> &'static str {
        "ocean_ruin"
    }

    fn find_generation_point<'a>(&'a self, context: &mut GenerationContext<'_>) -> Option<GenerationStub<'a>> {
        on_top_of_chunk_center(context, HeightmapType::OceanFloorWg, move |builder, context| self.generate_pieces(builder, context))
    }
}

#[linkme::distributed_slice(STRUCTURE_TYPES)]
static OCEAN_RUIN: TypeEntry<dyn Structure> = TypeEntry { name: "ocean_ruin", decode: OceanRuinStructure::decode };
