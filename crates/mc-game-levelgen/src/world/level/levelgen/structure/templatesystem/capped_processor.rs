//! `CappedProcessor` (`capped`): applies its delegate to at most `limit` randomly chosen blocks.

use mc_core::pos::BlockPos;
use mc_core::random::{self, RandomSource};
use mc_game_core::world::level::levelgen::codec::*;
use mc_game_core::world::level::levelgen::structure::templatesystem::structure_place_settings::StructurePlaceSettings;
use mc_game_core::world::level::levelgen::structure::templatesystem::structure_processor::{
    STRUCTURE_PROCESSOR_TYPES, StructureProcessor, decode_single,
};
use mc_game_core::world::level::levelgen::structure::templatesystem::structure_template::StructureBlockInfo;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

pub struct CappedProcessor {
    delegate: Arc<dyn StructureProcessor>,
    limit: IntProvider,
}

impl CappedProcessor {
    pub fn new(delegate: Arc<dyn StructureProcessor>, limit: IntProvider) -> CappedProcessor {
        CappedProcessor { delegate, limit }
    }
}

impl StructureProcessor for CappedProcessor {
    fn evaluates_entire_piece_state(&self) -> bool {
        true
    }

    fn finalize_processing(
        &self,
        level: &dyn WorldGenLevel,
        position: BlockPos,
        reference_pos: BlockPos,
        original_block_info_list: &[StructureBlockInfo],
        mut processed_block_info_list: Vec<StructureBlockInfo>,
        settings: &StructurePlaceSettings,
    ) -> Vec<StructureBlockInfo> {
        if self.limit.max_inclusive() == 0 || processed_block_info_list.is_empty() {
            return processed_block_info_list;
        }
        if original_block_info_list.len() != processed_block_info_list.len() {
            log::error!(
                "Original block info list not in sync with processed list, skipping processing. Original size: {}, Processed size: {}",
                original_block_info_list.len(),
                processed_block_info_list.len()
            );
            return processed_block_info_list;
        }
        let mut random = random::create_thread_local_instance(level.get_seed())
            .fork_positional()
            .at_pos(position);
        let max_to_replace = self
            .limit
            .sample(&mut *random)
            .min(processed_block_info_list.len() as i32);
        if max_to_replace < 1 {
            return processed_block_info_list;
        }
        let indices = to_shuffled_indices(processed_block_info_list.len(), &mut *random);
        let mut replaced = 0;
        for index in indices {
            if replaced >= max_to_replace {
                break;
            }
            let original = &original_block_info_list[index];
            let processed = &processed_block_info_list[index];
            let maybe_altered = self.delegate.process_block(
                level,
                position,
                reference_pos,
                original.pos,
                processed.clone(),
                settings,
            );
            if let Some(altered) = maybe_altered
                && *processed != altered
            {
                replaced += 1;
                processed_block_info_list[index] = altered;
            }
        }
        processed_block_info_list
    }
}

/// `Util.toShuffledList(IntStream.range(0, size), random)`.
fn to_shuffled_indices(size: usize, random: &mut dyn RandomSource) -> Vec<usize> {
    let mut result: Vec<usize> = (0..size).collect();
    let mut i = size;
    while i > 1 {
        let swap_to = random.next_int_bounded(i as i32) as usize;
        result.swap(i - 1, swap_to);
        i -= 1;
    }
    result
}

#[linkme::distributed_slice(STRUCTURE_PROCESSOR_TYPES)]
static TYPE: TypeEntry<dyn StructureProcessor> = TypeEntry {
    name: "capped",
    decode: |f, ctx| {
        let delegate = f.required("delegate", |v| decode_single(v, ctx))?;
        let limit = f.required("limit", IntProvider::positive_from_json)?;
        Ok(Arc::new(CappedProcessor::new(delegate, limit)))
    },
};
