//! `ProcessorRule`.

use super::pos_always_true_test::PosAlwaysTrueTest;
use super::pos_rule_test::{self, PosRuleTest};
use super::rule::blockentity::passthrough::Passthrough;
use super::rule::blockentity::rule_block_entity_modifier::{self, RuleBlockEntityModifier};
use super::rule_test::{self, RuleTest};
use mc_game_core::world::level::LevelReader;
use mc_game_core::world::level::levelgen::codec::*;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_nbt::CompoundTag;
use serde_json::Value;
use std::sync::Arc;

pub struct ProcessorRule {
    input_predicate: Arc<dyn RuleTest>,
    loc_predicate: Arc<dyn RuleTest>,
    pos_predicate: Arc<dyn PosRuleTest>,
    output_state: BlockState,
    block_entity_modifier: Arc<dyn RuleBlockEntityModifier>,
}

impl ProcessorRule {
    pub fn new(
        input_predicate: Arc<dyn RuleTest>,
        loc_predicate: Arc<dyn RuleTest>,
        pos_predicate: Arc<dyn PosRuleTest>,
        output_state: BlockState,
        block_entity_modifier: Arc<dyn RuleBlockEntityModifier>,
    ) -> ProcessorRule {
        ProcessorRule { input_predicate, loc_predicate, pos_predicate, output_state, block_entity_modifier }
    }

    /// `ProcessorRule.CODEC`; the two `lenientOptionalFieldOf` fields fall back to their defaults on any error.
    pub fn decode(value: &Value, ctx: &DecodeCtx<'_>) -> LoadResult<ProcessorRule> {
        let f = Fields::new(value)?;
        Ok(ProcessorRule {
            input_predicate: f.required("input_predicate", |v| rule_test::decode(v, ctx))?,
            loc_predicate: f.required("location_predicate", |v| rule_test::decode(v, ctx))?,
            pos_predicate: f
                .get("position_predicate")
                .and_then(|v| pos_rule_test::decode(v, ctx).ok())
                .unwrap_or_else(|| Arc::new(PosAlwaysTrueTest)),
            output_state: f.required("output_state", parse_block_state)?,
            block_entity_modifier: f
                .get("block_entity_modifier")
                .and_then(|v| rule_block_entity_modifier::decode(v, ctx).ok())
                .unwrap_or_else(|| Arc::new(Passthrough)),
        })
    }

    /// `test(level, inputState, inTemplatePos, worldPos, reference, random)`.
    pub fn test(&self, level: &dyn LevelReader, input_state: BlockState, in_template_pos: BlockPos, world_pos: BlockPos, reference: BlockPos, random: &mut dyn RandomSource) -> bool {
        self.input_predicate.test(input_state, world_pos, random)
            && self.loc_predicate.test_against_world_state(level, world_pos, random)
            && self.pos_predicate.test(in_template_pos, world_pos, reference, random)
    }

    pub fn get_output_state(&self) -> BlockState {
        self.output_state
    }

    /// `getOutputTag(random, existingTag)`.
    pub fn get_output_tag(&self, random: &mut dyn RandomSource, existing_tag: Option<&CompoundTag>) -> Option<CompoundTag> {
        self.block_entity_modifier.apply(random, existing_tag)
    }
}
