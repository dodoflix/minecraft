//! `RuleProcessor` (`rule`).

use super::processor_rule::ProcessorRule;
use mc_game_core::world::level::levelgen::structure::templatesystem::structure_place_settings::StructurePlaceSettings;
use mc_game_core::world::level::levelgen::structure::templatesystem::structure_processor::{STRUCTURE_PROCESSOR_TYPES, StructureProcessor};
use mc_game_core::world::level::levelgen::structure::templatesystem::structure_template::StructureBlockInfo;
use mc_game_core::world::level::LevelReader;
use mc_game_core::world::level::levelgen::codec::*;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random;
use std::sync::Arc;

pub struct RuleProcessor {
    rules: Vec<ProcessorRule>,
}

impl RuleProcessor {
    pub fn new(rules: Vec<ProcessorRule>) -> RuleProcessor {
        RuleProcessor { rules }
    }
}

impl StructureProcessor for RuleProcessor {
    fn process_block(
        &self,
        level: &dyn LevelReader,
        _target_position: BlockPos,
        reference_pos: BlockPos,
        template_relative_pos: BlockPos,
        processed_block_info: StructureBlockInfo,
        _settings: &StructurePlaceSettings,
    ) -> Option<StructureBlockInfo> {
        let mut random = random::create(mth::get_seed(&processed_block_info.pos));
        for rule in &self.rules {
            if rule.test(level, processed_block_info.state, template_relative_pos, processed_block_info.pos, reference_pos, &mut random) {
                let nbt = rule.get_output_tag(&mut random, processed_block_info.nbt.as_ref());
                return Some(StructureBlockInfo::new(processed_block_info.pos, rule.get_output_state(), nbt));
            }
        }
        Some(processed_block_info)
    }
}

#[linkme::distributed_slice(STRUCTURE_PROCESSOR_TYPES)]
static TYPE: TypeEntry<dyn StructureProcessor> = TypeEntry {
    name: "rule",
    decode: |f, ctx| {
        let rules = f.required("rules", |v| parse_list(v, |v| ProcessorRule::decode(v, ctx)))?;
        Ok(Arc::new(RuleProcessor::new(rules)))
    },
};
