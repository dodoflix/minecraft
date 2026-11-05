//! `GravityProcessor` (`gravity`).
//!
//! Vanilla maps `WORLD_SURFACE_WG`/`OCEAN_FLOOR_WG` to their non-`_WG` heightmaps when the level is a `ServerLevel`;
//! templates are only ever processed against world generation regions here, where the configured heightmap is used.

use super::structure_place_settings::StructurePlaceSettings;
use super::structure_processor::{STRUCTURE_PROCESSOR_TYPES, StructureProcessor};
use super::structure_template::StructureBlockInfo;
use crate::world::level::LevelReader;
use crate::world::level::levelgen::codec::*;
use mc_core::pos::BlockPos;
use mc_world::heightmap::HeightmapType;
use std::sync::Arc;

pub struct GravityProcessor {
    pub heightmap: HeightmapType,
    pub offset: i32,
}

impl StructureProcessor for GravityProcessor {
    fn process_block(
        &self,
        level: &dyn LevelReader,
        _target_position: BlockPos,
        _reference_pos: BlockPos,
        template_relative_pos: BlockPos,
        processed_block_info: StructureBlockInfo,
        _settings: &StructurePlaceSettings,
    ) -> Option<StructureBlockInfo> {
        let pos = processed_block_info.pos;
        let height = level.get_height_at(self.heightmap, pos.x(), pos.z()).wrapping_add(self.offset);
        let delta = template_relative_pos.y();
        Some(StructureBlockInfo::new(BlockPos::new(pos.x(), height.wrapping_add(delta), pos.z()), processed_block_info.state, processed_block_info.nbt))
    }
}

/// `Heightmap.Types.CODEC`.
fn parse_heightmap(value: &serde_json::Value) -> LoadResult<HeightmapType> {
    let name = parse_str(value)?;
    HeightmapType::from_serialization_key(name).ok_or_else(|| LoadError::new(format!("Unknown element name:{name}")))
}

#[linkme::distributed_slice(STRUCTURE_PROCESSOR_TYPES)]
static TYPE: TypeEntry<dyn StructureProcessor> = TypeEntry {
    name: "gravity",
    decode: |f, _| {
        Ok(Arc::new(GravityProcessor {
            heightmap: f.optional("heightmap", parse_heightmap)?.unwrap_or(HeightmapType::WorldSurfaceWg),
            offset: f.optional("offset", parse_int)?.unwrap_or(0),
        }))
    },
};
