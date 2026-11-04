//! `net.minecraft.world.level.levelgen.heightproviders.*`.
//!
//! Vanilla's "Empty height range" log warnings (`LOGGER.warn`, de-duplicated for `UniformHeight`) are not
//! reproduced; only the returned values are.

use serde_json::Value;

use mc_core::math::mth;
use mc_core::random::RandomSource;

use super::weighted::WeightedList;
use super::{dispatch_type, unknown_type};
use crate::densityfunction::codec::{LoadResult, parse_int, parse_int_range};
use crate::vertical_anchor::VerticalAnchor;
use crate::world_generation_context::WorldGenerationContext;

#[derive(Clone, Debug, PartialEq)]
pub enum HeightProvider {
    Constant(VerticalAnchor),
    Uniform { min_inclusive: VerticalAnchor, max_inclusive: VerticalAnchor },
    BiasedToBottom { min_inclusive: VerticalAnchor, max_inclusive: VerticalAnchor, inner: i32 },
    VeryBiasedToBottom { min_inclusive: VerticalAnchor, max_inclusive: VerticalAnchor, inner: i32 },
    Trapezoid { min_inclusive: VerticalAnchor, max_inclusive: VerticalAnchor, plateau: i32 },
    WeightedList(WeightedList<HeightProvider>),
}

impl HeightProvider {
    /// `HeightProvider.sample(random, context)`.
    pub fn sample(&self, random: &mut dyn RandomSource, context: &WorldGenerationContext) -> i32 {
        match self {
            HeightProvider::Constant(value) => value.resolve_y(context),
            HeightProvider::Uniform { min_inclusive, max_inclusive } => {
                let min = min_inclusive.resolve_y(context);
                let max = max_inclusive.resolve_y(context);
                if min > max {
                    min
                } else {
                    mth::random_between_inclusive(random, min, max)
                }
            }
            HeightProvider::BiasedToBottom { min_inclusive, max_inclusive, inner } => {
                let min = min_inclusive.resolve_y(context);
                let max = max_inclusive.resolve_y(context);
                let bound = max.wrapping_sub(min).wrapping_sub(*inner).wrapping_add(1);
                if bound <= 0 {
                    min
                } else {
                    let limit = random.next_int_bounded(bound);
                    random.next_int_bounded(limit.wrapping_add(*inner)).wrapping_add(min)
                }
            }
            HeightProvider::VeryBiasedToBottom { min_inclusive, max_inclusive, inner } => {
                let min = min_inclusive.resolve_y(context);
                let max = max_inclusive.resolve_y(context);
                if max.wrapping_sub(min).wrapping_sub(*inner).wrapping_add(1) <= 0 {
                    min
                } else {
                    let upper_inclusive = mth::next_int(random, min.wrapping_add(*inner), max);
                    let biased_upper_inclusive =
                        mth::next_int(random, min, upper_inclusive.wrapping_sub(1));
                    mth::next_int(
                        random,
                        min,
                        biased_upper_inclusive.wrapping_sub(1).wrapping_add(*inner),
                    )
                }
            }
            HeightProvider::Trapezoid { min_inclusive, max_inclusive, plateau } => {
                let min = min_inclusive.resolve_y(context);
                let max = max_inclusive.resolve_y(context);
                if min > max {
                    min
                } else {
                    let range = max.wrapping_sub(min);
                    if *plateau >= range {
                        mth::random_between_inclusive(random, min, max)
                    } else {
                        let plateau_start = range.wrapping_sub(*plateau) / 2;
                        let plateau_end = range.wrapping_sub(plateau_start);
                        let first = mth::random_between_inclusive(random, 0, plateau_end);
                        let second = mth::random_between_inclusive(random, 0, plateau_start);
                        min.wrapping_add(first).wrapping_add(second)
                    }
                }
            }
            HeightProvider::WeightedList(distribution) => {
                distribution.get_random_or_throw(random).sample(random, context)
            }
        }
    }

    /// `HeightProvider.CODEC`: a vertical anchor object is a constant, otherwise dispatched on `type`.
    pub fn from_json(value: &Value) -> LoadResult<HeightProvider> {
        // `Codec.either(VerticalAnchor.CODEC, dispatch)`: the anchor is tried first.
        if let Ok(anchor) = VerticalAnchor::from_json(value) {
            return Ok(HeightProvider::Constant(anchor));
        }
        let (ty, f) = dispatch_type(value)?;
        let anchors = || -> LoadResult<(VerticalAnchor, VerticalAnchor)> {
            let min = f.required("min_inclusive", VerticalAnchor::from_json)?;
            let max = f.required("max_inclusive", VerticalAnchor::from_json)?;
            Ok((min, max))
        };
        let inner = || -> LoadResult<i32> {
            Ok(f.optional("inner", |v| parse_int_range(v, 1, i32::MAX))?.unwrap_or(1))
        };
        match ty.as_str() {
            // `VerticalAnchor.CODEC.fieldOf("value")`
            "constant" => Ok(HeightProvider::Constant(
                f.required("value", VerticalAnchor::from_json)?,
            )),
            "uniform" => {
                let (min_inclusive, max_inclusive) = anchors()?;
                Ok(HeightProvider::Uniform { min_inclusive, max_inclusive })
            }
            "biased_to_bottom" => {
                let (min_inclusive, max_inclusive) = anchors()?;
                Ok(HeightProvider::BiasedToBottom { min_inclusive, max_inclusive, inner: inner()? })
            }
            "very_biased_to_bottom" => {
                let (min_inclusive, max_inclusive) = anchors()?;
                Ok(HeightProvider::VeryBiasedToBottom {
                    min_inclusive,
                    max_inclusive,
                    inner: inner()?,
                })
            }
            "trapezoid" => {
                let (min_inclusive, max_inclusive) = anchors()?;
                let plateau = f.optional("plateau", parse_int)?.unwrap_or(0);
                Ok(HeightProvider::Trapezoid { min_inclusive, max_inclusive, plateau })
            }
            "weighted_list" => Ok(HeightProvider::WeightedList(f.required("distribution", |v| {
                WeightedList::non_empty_from_json(v, HeightProvider::from_json)
            })?)),
            other => Err(unknown_type(other)),
        }
    }
}

impl From<VerticalAnchor> for HeightProvider {
    /// `ConstantHeight.of(anchor)`.
    fn from(anchor: VerticalAnchor) -> HeightProvider {
        HeightProvider::Constant(anchor)
    }
}
