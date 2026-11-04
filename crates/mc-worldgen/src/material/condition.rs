//! `levelgen/material/condition/*`: the `MaterialCondition` types, their codecs and their compiled
//! (`ConditionEvaluator`) form with the lazy per-XZ / per-Y result caches.

use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::random::PositionalRandomFactory;
use serde_json::Value;

use super::cave_surface::CaveSurface;
use super::context::MaterialRuleContext;
use super::{Decoder, MATERIAL_CONDITION_REGISTRY, parse_noise_key, parse_type};
use crate::data::LoadError;
use crate::densityfunction::codec::{
    Fields, LoadResult, parse_bool, parse_f64, parse_identifier, parse_int, parse_int_range,
};
use crate::vertical_anchor::VerticalAnchor;
use mc_world::palette::BiomeId;

/// `MaterialCondition`: `HolderHolder` is [`MaterialCondition::Reference`].
#[derive(Clone, Debug)]
pub enum MaterialCondition {
    /// `MaterialCondition.HolderHolder`: a reference into `worldgen/material_condition`.
    Reference { key: Identifier, condition: Arc<MaterialCondition> },
    /// `BiomeCondition`: `biome_is` flattened to the member biomes.
    Biome { biomes: Vec<BiomeId> },
    NoiseThreshold { noise: Identifier, min_threshold: f64, max_threshold: f64, is_3d: bool },
    VerticalGradient {
        random_name: Identifier,
        true_at_and_below: VerticalAnchor,
        false_at_and_above: VerticalAnchor,
    },
    /// `YCondition` (`y_above`).
    Y { anchor: VerticalAnchor, surface_depth_multiplier: i32, add_stone_depth: bool },
    Water { offset: i32, surface_depth_multiplier: i32, add_stone_depth: bool },
    Temperature,
    Steep,
    Not(Box<MaterialCondition>),
    Hole,
    AbovePreliminarySurface,
    StoneDepth {
        offset: i32,
        add_surface_depth: bool,
        secondary_depth_range: i32,
        surface_type: CaveSurface,
    },
}

impl MaterialCondition {
    /// `MaterialCondition.CODEC`: a string is a reference to a `material_condition` entry, an object is an
    /// inline condition.
    pub fn from_json(
        value: &Value,
        registries: &crate::data::WorldgenRegistries,
        biomes: &crate::biome::BiomeRegistry,
    ) -> LoadResult<MaterialCondition> {
        Self::decode(value, &Decoder { registries, biomes })
    }

    /// `MaterialCondition.DIRECT_CODEC`.
    pub fn direct_from_json(
        value: &Value,
        registries: &crate::data::WorldgenRegistries,
        biomes: &crate::biome::BiomeRegistry,
    ) -> LoadResult<MaterialCondition> {
        Self::decode_direct(value, &Decoder { registries, biomes })
    }

    pub(super) fn decode(value: &Value, d: &Decoder) -> LoadResult<MaterialCondition> {
        match value {
            Value::String(_) => {
                let key = parse_identifier(value)?;
                let condition = d.reference(MATERIAL_CONDITION_REGISTRY, &key, |json| {
                    MaterialCondition::decode_direct(json, d)
                })?;
                Ok(MaterialCondition::Reference { key, condition })
            }
            _ => Self::decode_direct(value, d),
        }
    }

    pub(super) fn decode_direct(value: &Value, d: &Decoder) -> LoadResult<MaterialCondition> {
        let fields = Fields::new(value)?;
        let ty = fields.required("type", parse_type)?;
        match ty.as_str() {
            "biome" => Ok(MaterialCondition::Biome {
                biomes: fields.required("biome_is", |v| d.biomes.holder_set_from_json(v))?,
            }),
            "noise_threshold" => Ok(MaterialCondition::NoiseThreshold {
                noise: fields.required("noise", |v| parse_noise_key(v, d))?,
                min_threshold: fields.required("min_threshold", parse_f64)?,
                max_threshold: fields.required("max_threshold", parse_f64)?,
                is_3d: fields.optional("is_3d", parse_bool)?.unwrap_or(false),
            }),
            "vertical_gradient" => Ok(MaterialCondition::VerticalGradient {
                random_name: fields.required("random_name", parse_identifier)?,
                true_at_and_below: fields.required("true_at_and_below", VerticalAnchor::from_json)?,
                false_at_and_above: fields
                    .required("false_at_and_above", VerticalAnchor::from_json)?,
            }),
            "y_above" => Ok(MaterialCondition::Y {
                anchor: fields.required("anchor", VerticalAnchor::from_json)?,
                surface_depth_multiplier: fields
                    .required("surface_depth_multiplier", |v| parse_int_range(v, -20, 20))?,
                add_stone_depth: fields.required("add_stone_depth", parse_bool)?,
            }),
            "water" => Ok(MaterialCondition::Water {
                offset: fields.required("offset", parse_int)?,
                surface_depth_multiplier: fields
                    .required("surface_depth_multiplier", |v| parse_int_range(v, -20, 20))?,
                add_stone_depth: fields.required("add_stone_depth", parse_bool)?,
            }),
            "temperature" => Ok(MaterialCondition::Temperature),
            "steep" => Ok(MaterialCondition::Steep),
            "not" => Ok(MaterialCondition::Not(Box::new(
                fields.required("invert", |v| MaterialCondition::decode(v, d))?,
            ))),
            "hole" => Ok(MaterialCondition::Hole),
            "above_preliminary_surface" => Ok(MaterialCondition::AbovePreliminarySurface),
            "stone_depth" => Ok(MaterialCondition::StoneDepth {
                offset: fields.required("offset", parse_int)?,
                add_surface_depth: fields.required("add_surface_depth", parse_bool)?,
                secondary_depth_range: fields.required("secondary_depth_range", parse_int)?,
                surface_type: fields.required("surface_type", CaveSurface::from_json)?,
            }),
            other => Err(LoadError::new(format!("Unknown registry key: minecraft:{other}")).at("type")),
        }
    }

    /// `compile(context)`.
    pub(crate) fn compile<'a>(&self, ctx: &mut MaterialRuleContext<'a>) -> ConditionEvaluator {
        match self {
            MaterialCondition::Reference { condition, .. } => condition.compile(ctx),
            MaterialCondition::Biome { biomes } => {
                if let Some(possible) = ctx.possible_biomes() {
                    // canNeverMatch: no member of the condition is a possible biome.
                    if !biomes.iter().any(|biome| possible.contains(biome)) {
                        return ConditionEvaluator::Const(false);
                    }
                    // willAlwaysMatch: every possible biome is a member.
                    if possible.iter().all(|biome| biomes.contains(biome)) {
                        return ConditionEvaluator::Const(true);
                    }
                }
                ConditionEvaluator::Biome { biomes: biomes.clone(), cache: Lazy::y(ctx) }
            }
            MaterialCondition::NoiseThreshold { noise, min_threshold, max_threshold, is_3d } => {
                ConditionEvaluator::NoiseThreshold {
                    sampler: ctx.get_noise_sampler(noise, *is_3d),
                    min_threshold: *min_threshold,
                    max_threshold: *max_threshold,
                }
            }
            MaterialCondition::VerticalGradient { random_name, true_at_and_below, false_at_and_above } => {
                ConditionEvaluator::VerticalGradient {
                    true_at_and_below: ctx.resolve_anchor_y(true_at_and_below),
                    false_at_and_above: ctx.resolve_anchor_y(false_at_and_above),
                    random_factory: ctx.get_or_create_random_factory(random_name),
                    cache: Lazy::y(ctx),
                }
            }
            MaterialCondition::Y { anchor, surface_depth_multiplier, add_stone_depth } => {
                ConditionEvaluator::Y {
                    anchor: *anchor,
                    surface_depth_multiplier: *surface_depth_multiplier,
                    add_stone_depth: *add_stone_depth,
                    cache: Lazy::y(ctx),
                }
            }
            MaterialCondition::Water { offset, surface_depth_multiplier, add_stone_depth } => {
                ConditionEvaluator::Water {
                    offset: *offset,
                    surface_depth_multiplier: *surface_depth_multiplier,
                    add_stone_depth: *add_stone_depth,
                    cache: Lazy::y(ctx),
                }
            }
            MaterialCondition::Temperature => ConditionEvaluator::Temperature,
            MaterialCondition::Steep => ConditionEvaluator::Steep { cache: Lazy::xz(ctx) },
            MaterialCondition::Not(target) => {
                ConditionEvaluator::Not(Box::new(target.compile(ctx)))
            }
            MaterialCondition::Hole => ConditionEvaluator::Hole { cache: Lazy::xz(ctx) },
            MaterialCondition::AbovePreliminarySurface => ConditionEvaluator::AbovePreliminarySurface,
            MaterialCondition::StoneDepth {
                offset,
                add_surface_depth,
                secondary_depth_range,
                surface_type,
            } => ConditionEvaluator::StoneDepth {
                offset: *offset,
                add_surface_depth: *add_surface_depth,
                secondary_depth_range: *secondary_depth_range,
                ceiling: *surface_type == CaveSurface::Ceiling,
                cache: Lazy::y(ctx),
            },
        }
    }
}

/// The result cache of `LazyXZCondition` / `LazyYCondition`: valid while the context's matching update
/// counter equals `last_update`.
pub(crate) struct Lazy {
    last_update: i64,
    result: bool,
}

impl Lazy {
    /// `LazyXZCondition(context)`.
    fn xz(ctx: &MaterialRuleContext) -> Lazy {
        Lazy { last_update: ctx.last_update_xz().wrapping_sub(1), result: false }
    }

    /// `LazyYCondition(context)`.
    fn y(ctx: &MaterialRuleContext) -> Lazy {
        Lazy { last_update: ctx.last_update_y().wrapping_sub(1), result: false }
    }

    fn get(
        &mut self,
        counter: i64,
        ctx: &mut MaterialRuleContext,
        compute: impl FnOnce(&mut MaterialRuleContext) -> bool,
    ) -> bool {
        if counter != self.last_update {
            self.last_update = counter;
            self.result = compute(ctx);
        }
        self.result
    }
}

/// `ConditionEvaluator`: a condition compiled against a [`MaterialRuleContext`].
pub(crate) enum ConditionEvaluator {
    /// `() -> true` / `() -> false`.
    Const(bool),
    Biome { biomes: Vec<BiomeId>, cache: Lazy },
    NoiseThreshold { sampler: usize, min_threshold: f64, max_threshold: f64 },
    VerticalGradient {
        true_at_and_below: i32,
        false_at_and_above: i32,
        random_factory: Arc<dyn PositionalRandomFactory>,
        cache: Lazy,
    },
    Y { anchor: VerticalAnchor, surface_depth_multiplier: i32, add_stone_depth: bool, cache: Lazy },
    Water { offset: i32, surface_depth_multiplier: i32, add_stone_depth: bool, cache: Lazy },
    Temperature,
    Steep { cache: Lazy },
    Not(Box<ConditionEvaluator>),
    Hole { cache: Lazy },
    AbovePreliminarySurface,
    StoneDepth {
        offset: i32,
        add_surface_depth: bool,
        secondary_depth_range: i32,
        ceiling: bool,
        cache: Lazy,
    },
}

impl ConditionEvaluator {
    /// `test()`.
    pub(crate) fn test(&mut self, ctx: &mut MaterialRuleContext) -> bool {
        match self {
            ConditionEvaluator::Const(value) => *value,
            ConditionEvaluator::Biome { biomes, cache } => {
                let counter = ctx.last_update_y();
                cache.get(counter, ctx, |ctx| biomes.contains(&ctx.get_biome()))
            }
            ConditionEvaluator::NoiseThreshold { sampler, min_threshold, max_threshold } => {
                let value = ctx.noise_sampler_value(*sampler);
                value >= *min_threshold && value <= *max_threshold
            }
            ConditionEvaluator::VerticalGradient {
                true_at_and_below,
                false_at_and_above,
                random_factory,
                cache,
            } => {
                let counter = ctx.last_update_y();
                cache.get(counter, ctx, |ctx| {
                    let block_y = ctx.block_y();
                    if block_y <= *true_at_and_below {
                        true
                    } else if block_y >= *false_at_and_above {
                        false
                    } else {
                        let probability = map(
                            f64::from(block_y),
                            f64::from(*true_at_and_below),
                            f64::from(*false_at_and_above),
                            1.0,
                            0.0,
                        );
                        let mut random = random_factory.at(ctx.block_x(), block_y, ctx.block_z());
                        f64::from(random.next_float()) < probability
                    }
                })
            }
            ConditionEvaluator::Y { anchor, surface_depth_multiplier, add_stone_depth, cache } => {
                let counter = ctx.last_update_y();
                cache.get(counter, ctx, |ctx| {
                    let stone_depth = if *add_stone_depth { ctx.stone_depth_above() } else { 0 };
                    ctx.block_y().wrapping_add(stone_depth)
                        >= ctx
                            .resolve_anchor_y(anchor)
                            .wrapping_add(ctx.surface_depth().wrapping_mul(*surface_depth_multiplier))
                })
            }
            ConditionEvaluator::Water { offset, surface_depth_multiplier, add_stone_depth, cache } => {
                let counter = ctx.last_update_y();
                cache.get(counter, ctx, |ctx| {
                    if ctx.water_height() == i32::MIN {
                        return true;
                    }
                    let stone_depth = if *add_stone_depth { ctx.stone_depth_above() } else { 0 };
                    ctx.block_y().wrapping_add(stone_depth)
                        >= ctx
                            .water_height()
                            .wrapping_add(*offset)
                            .wrapping_add(ctx.surface_depth().wrapping_mul(*surface_depth_multiplier))
                })
            }
            ConditionEvaluator::Temperature => {
                let biome = ctx.get_biome();
                let pos = ctx.block_pos();
                let sea_level = ctx.sea_level();
                ctx.biome_data(biome).cold_enough_to_snow(pos, sea_level)
            }
            ConditionEvaluator::Steep { cache } => {
                let counter = ctx.last_update_xz();
                cache.get(counter, ctx, |ctx| {
                    ctx.surface_gradient_x() <= -4 || ctx.surface_gradient_z() >= 4
                })
            }
            ConditionEvaluator::Not(target) => !target.test(ctx),
            ConditionEvaluator::Hole { cache } => {
                let counter = ctx.last_update_xz();
                cache.get(counter, ctx, |ctx| ctx.surface_depth() <= 0)
            }
            ConditionEvaluator::AbovePreliminarySurface => {
                ctx.block_y() >= ctx.get_min_surface_level()
            }
            ConditionEvaluator::StoneDepth {
                offset,
                add_surface_depth,
                secondary_depth_range,
                ceiling,
                cache,
            } => {
                let counter = ctx.last_update_y();
                cache.get(counter, ctx, |ctx| {
                    let stone_depth =
                        if *ceiling { ctx.stone_depth_below() } else { ctx.stone_depth_above() };
                    let surface_depth = if *add_surface_depth { ctx.surface_depth() } else { 0 };
                    let secondary_surface_depth = if *secondary_depth_range == 0 {
                        0
                    } else {
                        map(ctx.get_surface_secondary(), -1.0, 1.0, 0.0, f64::from(*secondary_depth_range))
                            as i32
                    };
                    stone_depth
                        <= 1i32
                            .wrapping_add(*offset)
                            .wrapping_add(surface_depth)
                            .wrapping_add(secondary_surface_depth)
                })
            }
        }
    }
}

/// `Mth.map(double, double, double, double, double)`.
fn map(value: f64, from_min: f64, from_max: f64, to_min: f64, to_max: f64) -> f64 {
    mth::lerp_f64(mth::inverse_lerp_f64(value, from_min, from_max), to_min, to_max)
}
