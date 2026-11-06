//! `levelgen/material/rule/*`: the `MaterialRule` types, their codecs and compiled (`RuleEvaluator`) form.

use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_core::random::PositionalRandomFactory;
use mc_data::block::BlockState;
use serde_json::Value;

use super::condition::{ConditionEvaluator, MaterialCondition};
use super::context::{DensityGetter, MaterialRuleContext};
use super::{Decoder, MATERIAL_RULE_REGISTRY, parse_type};
use crate::data::LoadError;
use crate::densityfunction::DensityFunction;
use crate::densityfunction::codec::{
    Fields, LoadResult, density_function_from_json, parse_f32, parse_identifier, parse_list,
};
use crate::noise_settings::BlockStateSpec;

/// `OreVeinRule`.
#[derive(Clone, Debug)]
pub struct OreVeinRule {
    pub ore_block: BlockState,
    pub raw_ore_block: BlockState,
    pub filler_block: BlockState,
    pub raw_ore_chance: f32,
    pub density: DensityFunction,
    pub richness: DensityFunction,
    pub filler_gap: DensityFunction,
}

/// `MaterialRule`: `HolderHolder` is [`MaterialRule::Reference`].
#[derive(Clone, Debug)]
pub enum MaterialRule {
    /// `MaterialRule.HolderHolder`: a reference into `worldgen/material_rule`.
    Reference {
        key: Identifier,
        rule: Arc<MaterialRule>,
    },
    /// `BlockRule`.
    Block {
        result_state: BlockState,
    },
    /// `BandlandsRule`.
    Bandlands,
    /// `SequenceRule`.
    Sequence(Vec<MaterialRule>),
    /// `ConditionRule`.
    Condition {
        if_true: MaterialCondition,
        then_run: Box<MaterialRule>,
    },
    OreVein(Box<OreVeinRule>),
}

impl MaterialRule {
    /// `MaterialRule.CODEC` (`HOLDER_CODEC` unwrapped): a string is a reference to a `material_rule` entry, an
    /// object an inline rule.
    pub fn from_json(
        value: &Value,
        registries: &crate::data::WorldgenRegistries,
        biomes: &crate::biome::BiomeRegistry,
    ) -> LoadResult<MaterialRule> {
        Self::decode(value, &Decoder { registries, biomes })
    }

    /// `MaterialRule.DIRECT_CODEC`.
    pub fn direct_from_json(
        value: &Value,
        registries: &crate::data::WorldgenRegistries,
        biomes: &crate::biome::BiomeRegistry,
    ) -> LoadResult<MaterialRule> {
        Self::decode_direct(value, &Decoder { registries, biomes })
    }

    pub(super) fn decode(value: &Value, d: &Decoder) -> LoadResult<MaterialRule> {
        match value {
            Value::String(_) => {
                let key = parse_identifier(value)?;
                let rule = d.reference(MATERIAL_RULE_REGISTRY, &key, |json| {
                    MaterialRule::decode_direct(json, d)
                })?;
                Ok(MaterialRule::Reference { key, rule })
            }
            _ => Self::decode_direct(value, d),
        }
    }

    pub(super) fn decode_direct(value: &Value, d: &Decoder) -> LoadResult<MaterialRule> {
        let fields = Fields::new(value)?;
        let ty = fields.required("type", parse_type)?;
        match ty.as_str() {
            "block" => Ok(MaterialRule::Block {
                result_state: fields.required("result_state", block_state)?,
            }),
            "bandlands" => Ok(MaterialRule::Bandlands),
            "sequence" => Ok(MaterialRule::Sequence(
                fields.required("sequence", |v| {
                    parse_list(v, |e| MaterialRule::decode(e, d))
                })?,
            )),
            "condition" => Ok(MaterialRule::Condition {
                if_true: fields.required("if_true", |v| MaterialCondition::decode(v, d))?,
                then_run: Box::new(fields.required("then_run", |v| MaterialRule::decode(v, d))?),
            }),
            "ore_vein" => {
                let df = |name: &str| {
                    fields.required(name, |v| density_function_from_json(v, d.registries))
                };
                Ok(MaterialRule::OreVein(Box::new(OreVeinRule {
                    ore_block: fields.required("ore_block", block_state)?,
                    raw_ore_block: fields.required("raw_ore_block", block_state)?,
                    filler_block: fields.required("filler_block", block_state)?,
                    raw_ore_chance: fields.required("raw_ore_chance", |v| {
                        let chance = parse_f32(v)?;
                        if (0.0..=1.0).contains(&chance) {
                            Ok(chance)
                        } else {
                            Err(LoadError::new(format!(
                                "Value {chance} outside of range [0.0:1.0]"
                            )))
                        }
                    })?,
                    density: df("density")?,
                    richness: df("richness")?,
                    filler_gap: df("filler_gap")?,
                })))
            }
            other => {
                Err(LoadError::new(format!("Unknown registry key: minecraft:{other}")).at("type"))
            }
        }
    }

    /// `compile(context)`.
    pub(crate) fn compile<'a>(&self, ctx: &mut MaterialRuleContext<'a>) -> RuleEvaluator<'a> {
        match self {
            MaterialRule::Reference { rule, .. } => rule.compile(ctx),
            MaterialRule::Block { result_state } => RuleEvaluator::Block(*result_state),
            MaterialRule::Bandlands => RuleEvaluator::Bandlands,
            MaterialRule::Sequence(sequence) => {
                if sequence.len() == 1 {
                    sequence[0].compile(ctx)
                } else {
                    RuleEvaluator::Sequence(sequence.iter().map(|rule| rule.compile(ctx)).collect())
                }
            }
            MaterialRule::Condition { if_true, then_run } => {
                let if_true = if_true.compile(ctx);
                let then_run = then_run.compile(ctx);
                RuleEvaluator::Condition(Box::new((if_true, then_run)))
            }
            MaterialRule::OreVein(rule) => {
                let density = ctx.get_densities_in_chunk(&rule.density, true);
                let richness = ctx.get_densities_in_chunk(&rule.richness, true);
                let filler_gap = ctx.get_densities_in_chunk(&rule.filler_gap, false);
                let random_factory = ctx.get_or_create_random_factory(
                    &Identifier::with_default_namespace("ore").expect("valid identifier"),
                );
                RuleEvaluator::OreVein(Box::new(OreVeinEvaluator {
                    density,
                    richness,
                    filler_gap,
                    random_factory,
                    ore_block: rule.ore_block,
                    raw_ore_block: rule.raw_ore_block,
                    filler_block: rule.filler_block,
                    raw_ore_chance: rule.raw_ore_chance,
                }))
            }
        }
    }
}

/// `BlockState.CODEC`.
fn block_state(value: &Value) -> LoadResult<BlockState> {
    BlockStateSpec::from_json(value)?.resolve()
}

/// `RuleEvaluator`: a rule compiled against a [`MaterialRuleContext`].
pub(crate) enum RuleEvaluator<'a> {
    Block(BlockState),
    Bandlands,
    Sequence(Vec<RuleEvaluator<'a>>),
    /// `ConditionRule`: `(ifTrue, thenRun)`.
    Condition(Box<(ConditionEvaluator, RuleEvaluator<'a>)>),
    OreVein(Box<OreVeinEvaluator<'a>>),
}

pub(crate) struct OreVeinEvaluator<'a> {
    density: DensityGetter<'a>,
    richness: DensityGetter<'a>,
    filler_gap: DensityGetter<'a>,
    random_factory: Arc<dyn PositionalRandomFactory>,
    ore_block: BlockState,
    raw_ore_block: BlockState,
    filler_block: BlockState,
    raw_ore_chance: f32,
}

impl OreVeinEvaluator<'_> {
    /// The lambda `OreVeinRule.compile` returns (`SharedConstants.DEBUG_*` ore vein flags are off).
    fn try_apply(
        &self,
        ctx: &MaterialRuleContext,
        block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> Option<BlockState> {
        let density = self.density.get(ctx);
        if density <= 0.0 {
            return None;
        }
        let mut random = self.random_factory.at(block_x, block_y, block_z);
        if random.next_float() > density {
            return None;
        }
        let richness = self.richness.get(ctx);
        if random.next_float() < richness && self.filler_gap.get(ctx) < 0.0 {
            Some(if random.next_float() < self.raw_ore_chance {
                self.raw_ore_block
            } else {
                self.ore_block
            })
        } else {
            Some(self.filler_block)
        }
    }
}

impl RuleEvaluator<'_> {
    /// `tryApply(blockX, blockY, blockZ)`.
    pub(crate) fn try_apply(
        &mut self,
        ctx: &mut MaterialRuleContext,
        block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> Option<BlockState> {
        match self {
            RuleEvaluator::Block(state) => Some(*state),
            RuleEvaluator::Bandlands => Some(ctx.get_band(block_x, block_y, block_z)),
            RuleEvaluator::Sequence(sequence) => {
                for rule in sequence {
                    if let Some(state) = rule.try_apply(ctx, block_x, block_y, block_z) {
                        return Some(state);
                    }
                }
                None
            }
            RuleEvaluator::Condition(parts) => {
                let (if_true, then_run) = &mut **parts;
                if if_true.test(ctx) {
                    then_run.try_apply(ctx, block_x, block_y, block_z)
                } else {
                    None
                }
            }
            RuleEvaluator::OreVein(rule) => rule.try_apply(ctx, block_x, block_y, block_z),
        }
    }
}
