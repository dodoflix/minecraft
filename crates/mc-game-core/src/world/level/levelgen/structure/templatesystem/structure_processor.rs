//! `StructureProcessor` (+ `StructureProcessorType`): rewrites the blocks of a template while it is placed. Types
//! register in [`STRUCTURE_PROCESSOR_TYPES`] and dispatch on `processor_type`.

use super::structure_place_settings::StructurePlaceSettings;
use super::structure_processor_list::StructureProcessorList;
use super::structure_template::StructureBlockInfo;
use crate::world::level::LevelReader;
use crate::world::level::levelgen::codec::*;
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::pos::BlockPos;
use serde_json::Value;
use std::sync::Arc;

pub trait StructureProcessor: Send + Sync {
    /// `processBlock(...)`: `None` drops the block.
    fn process_block(
        &self,
        _level: &dyn LevelReader,
        _target_position: BlockPos,
        _reference_pos: BlockPos,
        _template_relative_pos: BlockPos,
        processed_block_info: StructureBlockInfo,
        _settings: &StructurePlaceSettings,
    ) -> Option<StructureBlockInfo> {
        Some(processed_block_info)
    }

    /// `finalizeProcessing(...)`.
    fn finalize_processing(
        &self,
        _level: &dyn WorldGenLevel,
        _position: BlockPos,
        _reference_pos: BlockPos,
        _original_block_info_list: &[StructureBlockInfo],
        processed_block_info_list: Vec<StructureBlockInfo>,
        _settings: &StructurePlaceSettings,
    ) -> Vec<StructureBlockInfo> {
        processed_block_info_list
    }

    /// `evaluatesEntirePieceState()`.
    fn evaluates_entire_piece_state(&self) -> bool {
        false
    }
}

#[linkme::distributed_slice]
pub static STRUCTURE_PROCESSOR_TYPES: [TypeEntry<dyn StructureProcessor>];

/// `StructureProcessorType.SINGLE_CODEC`.
pub fn decode_single(
    value: &Value,
    ctx: &DecodeCtx<'_>,
) -> LoadResult<Arc<dyn StructureProcessor>> {
    decode_dispatch_on("processor_type", value, ctx, &STRUCTURE_PROCESSOR_TYPES)
}

/// `StructureProcessorType.LIST_OBJECT_CODEC`: a JSON list of processors.
fn decode_list_object(value: &Value, ctx: &DecodeCtx<'_>) -> LoadResult<StructureProcessorList> {
    Ok(StructureProcessorList::new(parse_list(value, |v| {
        decode_single(v, ctx)
    })?))
}

/// `StructureProcessorType.DIRECT_CODEC`: `Codec.withAlternative({"processors": [...]}, [...])`.
pub fn decode_direct_list(
    value: &Value,
    ctx: &DecodeCtx<'_>,
) -> LoadResult<StructureProcessorList> {
    let first =
        Fields::new(value).and_then(|f| f.required("processors", |v| decode_list_object(v, ctx)));
    let first_error = match first {
        Ok(list) => return Ok(list),
        Err(e) => e,
    };
    match decode_list_object(value, ctx) {
        Ok(list) => Ok(list),
        Err(second_error) => Err(LoadError::new(format!(
            "Failed to parse either. First: {first_error}; Second: {second_error}"
        ))),
    }
}
