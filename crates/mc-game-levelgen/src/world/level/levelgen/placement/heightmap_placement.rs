//! `HeightmapPlacement`: the origin moved to the heightmap's surface (dropped at/below the world bottom).

use super::{PLACEMENT_MODIFIER_TYPES, PlacementContext, PlacementModifier};
use crate::world::level::levelgen::codec::{Fields, LoadResult, LoadError, TypeEntry, parse_str};
use crate::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_world::heightmap::HeightmapType;
use serde_json::Value;
use std::sync::Arc;

pub struct HeightmapPlacement {
    pub heightmap: HeightmapType,
}

/// `Heightmap.Types.CODEC` (`StringRepresentable.fromEnum`: exact serialized key).
pub fn parse_heightmap_type(value: &Value) -> LoadResult<HeightmapType> {
    let name = parse_str(value)?;
    HeightmapType::from_serialization_key(name).ok_or_else(|| LoadError::new(format!("Unknown element name:{name}")))
}

impl HeightmapPlacement {
    fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PlacementModifier>> {
        Ok(Arc::new(HeightmapPlacement { heightmap: fields.required("heightmap", parse_heightmap_type)? }))
    }
}

impl PlacementModifier for HeightmapPlacement {
    fn modify(&self, context: &PlacementContext<'_>, _random: &mut dyn RandomSource, origin: BlockPos, output: &mut Vec<BlockPos>) {
        let (x, z) = (origin.x(), origin.z());
        let height = context.get_height(self.heightmap, x, z);
        if height > context.get_min_y() {
            output.push(BlockPos::new(x, height, z));
        }
    }
}

#[linkme::distributed_slice(PLACEMENT_MODIFIER_TYPES)]
static HEIGHTMAP: TypeEntry<dyn PlacementModifier> = TypeEntry { name: "heightmap", decode: HeightmapPlacement::decode };
