//! `net.minecraft.util.valueproviders.{IntProvider, IntProviders, ConstantInt, UniformInt, BiasedToBottomInt,
//! VeryBiasedToBottomInt, ClampedInt, ClampedNormalInt, WeightedListInt, TrapezoidInt}`.

use serde_json::Value;

use mc_core::math::mth;
use mc_core::random::RandomSource;

use super::weighted::WeightedList;
use super::{dispatch_type, unknown_type};
use crate::data::LoadError;
use crate::densityfunction::codec::{LoadResult, parse_f32, parse_int};

#[derive(Clone, Debug, PartialEq)]
pub enum IntProvider {
    Constant(i32),
    Uniform {
        min_inclusive: i32,
        max_inclusive: i32,
    },
    BiasedToBottom {
        min_inclusive: i32,
        max_inclusive: i32,
    },
    VeryBiasedToBottom {
        min_inclusive: i32,
        max_inclusive: i32,
    },
    Clamped {
        source: Box<IntProvider>,
        min_inclusive: i32,
        max_inclusive: i32,
    },
    /// `WeightedListInt` (`min`/`max` are the extrema over the entries, computed once).
    WeightedList {
        distribution: WeightedList<IntProvider>,
        min_value: i32,
        max_value: i32,
    },
    ClampedNormal {
        mean: f32,
        deviation: f32,
        min_inclusive: i32,
        max_inclusive: i32,
    },
    Trapezoid {
        min_inclusive: i32,
        max_inclusive: i32,
        plateau: i32,
    },
}

impl IntProvider {
    /// `ConstantInt.of(value)`.
    pub fn constant(value: i32) -> IntProvider {
        IntProvider::Constant(value)
    }

    /// `WeightedListInt(distribution)`.
    pub fn weighted_list(distribution: WeightedList<IntProvider>) -> IntProvider {
        let mut min = i32::MAX;
        let mut max = i32::MIN;
        for entry in distribution.unwrap() {
            min = min.min(entry.value.min_inclusive());
            max = max.max(entry.value.max_inclusive());
        }
        IntProvider::WeightedList {
            distribution,
            min_value: min,
            max_value: max,
        }
    }

    /// `ClampedNormalInt.sample(random, mean, deviation, min, max)`.
    pub fn sample_clamped_normal(
        random: &mut dyn RandomSource,
        mean: f32,
        deviation: f32,
        min_inclusive: f32,
        max_inclusive: f32,
    ) -> i32 {
        mth::clamp_f32(
            mth::normal(random, mean, deviation),
            min_inclusive,
            max_inclusive,
        ) as i32
    }

    pub fn sample(&self, random: &mut dyn RandomSource) -> i32 {
        match self {
            IntProvider::Constant(value) => *value,
            IntProvider::Uniform {
                min_inclusive,
                max_inclusive,
            } => mth::random_between_inclusive(random, *min_inclusive, *max_inclusive),
            IntProvider::BiasedToBottom {
                min_inclusive,
                max_inclusive,
            } => {
                let bound = max_inclusive.wrapping_sub(*min_inclusive).wrapping_add(1);
                let inner = random.next_int_bounded(bound).wrapping_add(1);
                min_inclusive.wrapping_add(random.next_int_bounded(inner))
            }
            IntProvider::VeryBiasedToBottom {
                min_inclusive,
                max_inclusive,
            } => {
                let bound = max_inclusive.wrapping_sub(*min_inclusive).wrapping_add(1);
                let a = random.next_int_bounded(bound).wrapping_add(1);
                let b = random.next_int_bounded(a).wrapping_add(1);
                min_inclusive.wrapping_add(random.next_int_bounded(b))
            }
            IntProvider::Clamped {
                source,
                min_inclusive,
                max_inclusive,
            } => mth::clamp_i32(source.sample(random), *min_inclusive, *max_inclusive),
            IntProvider::WeightedList { distribution, .. } => {
                distribution.get_random_or_throw(random).sample(random)
            }
            IntProvider::ClampedNormal {
                mean,
                deviation,
                min_inclusive,
                max_inclusive,
            } => IntProvider::sample_clamped_normal(
                random,
                *mean,
                *deviation,
                *min_inclusive as f32,
                *max_inclusive as f32,
            ),
            IntProvider::Trapezoid {
                min_inclusive,
                max_inclusive,
                plateau,
            } => {
                if *plateau == 0 && *max_inclusive == min_inclusive.wrapping_neg() {
                    let bound = max_inclusive.wrapping_add(1);
                    random
                        .next_int_bounded(bound)
                        .wrapping_sub(random.next_int_bounded(bound))
                } else {
                    let range = max_inclusive.wrapping_sub(*min_inclusive);
                    if *plateau == range {
                        mth::random_between_inclusive(random, *min_inclusive, *max_inclusive)
                    } else {
                        let plateau_start = range.wrapping_sub(*plateau) / 2;
                        let plateau_end = range.wrapping_sub(plateau_start);
                        let first = mth::random_between_inclusive(random, 0, plateau_end);
                        let second = mth::random_between_inclusive(random, 0, plateau_start);
                        min_inclusive.wrapping_add(first).wrapping_add(second)
                    }
                }
            }
        }
    }

    pub fn min_inclusive(&self) -> i32 {
        match self {
            IntProvider::Constant(value) => *value,
            IntProvider::Uniform { min_inclusive, .. }
            | IntProvider::BiasedToBottom { min_inclusive, .. }
            | IntProvider::VeryBiasedToBottom { min_inclusive, .. }
            | IntProvider::ClampedNormal { min_inclusive, .. }
            | IntProvider::Trapezoid { min_inclusive, .. } => *min_inclusive,
            IntProvider::Clamped {
                source,
                min_inclusive,
                ..
            } => (*min_inclusive).max(source.min_inclusive()),
            IntProvider::WeightedList { min_value, .. } => *min_value,
        }
    }

    pub fn max_inclusive(&self) -> i32 {
        match self {
            IntProvider::Constant(value) => *value,
            IntProvider::Uniform { max_inclusive, .. }
            | IntProvider::BiasedToBottom { max_inclusive, .. }
            | IntProvider::VeryBiasedToBottom { max_inclusive, .. }
            | IntProvider::ClampedNormal { max_inclusive, .. }
            | IntProvider::Trapezoid { max_inclusive, .. } => *max_inclusive,
            IntProvider::Clamped {
                source,
                max_inclusive,
                ..
            } => (*max_inclusive).min(source.max_inclusive()),
            IntProvider::WeightedList { max_value, .. } => *max_value,
        }
    }

    /// `IntProviders.CODEC`: a number is a constant, an object is dispatched on its `type`.
    pub fn from_json(value: &Value) -> LoadResult<IntProvider> {
        if let Value::Number(_) = value {
            return Ok(IntProvider::Constant(parse_int(value)?));
        }
        // Either's right branch: its error replaces the (number) left error like DFU's "Failed to parse either".
        let (ty, f) = dispatch_type(value).map_err(|e| {
            if value.is_object() {
                e
            } else {
                LoadError::new("Failed to parse either: expected number or object")
            }
        })?;
        let min_max = |name_min: &str, name_max: &str| -> LoadResult<(i32, i32)> {
            let min = f.required(name_min, parse_int)?;
            let max = f.required(name_max, parse_int)?;
            Ok((min, max))
        };
        let ordered = |min: i32, max: i32| -> LoadResult<()> {
            if max < min {
                return Err(LoadError::new(format!(
                    "Max must be at least min, min_inclusive: {min}, max_inclusive: {max}"
                )));
            }
            Ok(())
        };
        match ty.as_str() {
            "constant" => Ok(IntProvider::Constant(f.required("value", parse_int)?)),
            "uniform" => {
                let (min_inclusive, max_inclusive) = min_max("min_inclusive", "max_inclusive")?;
                ordered(min_inclusive, max_inclusive)?;
                Ok(IntProvider::Uniform {
                    min_inclusive,
                    max_inclusive,
                })
            }
            "biased_to_bottom" => {
                let (min_inclusive, max_inclusive) = min_max("min_inclusive", "max_inclusive")?;
                ordered(min_inclusive, max_inclusive)?;
                Ok(IntProvider::BiasedToBottom {
                    min_inclusive,
                    max_inclusive,
                })
            }
            "very_biased_to_bottom" => {
                let (min_inclusive, max_inclusive) = min_max("min_inclusive", "max_inclusive")?;
                ordered(min_inclusive, max_inclusive)?;
                Ok(IntProvider::VeryBiasedToBottom {
                    min_inclusive,
                    max_inclusive,
                })
            }
            "clamped" => {
                let source = f.required("source", IntProvider::from_json)?;
                let (min_inclusive, max_inclusive) = min_max("min_inclusive", "max_inclusive")?;
                ordered(min_inclusive, max_inclusive)?;
                Ok(IntProvider::Clamped {
                    source: Box::new(source),
                    min_inclusive,
                    max_inclusive,
                })
            }
            "weighted_list" => {
                let distribution = f.required("distribution", |v| {
                    WeightedList::non_empty_from_json(v, IntProvider::from_json)
                })?;
                Ok(IntProvider::weighted_list(distribution))
            }
            "clamped_normal" => {
                let mean = f.required("mean", parse_f32)?;
                let deviation = f.required("deviation", parse_f32)?;
                let (min_inclusive, max_inclusive) = min_max("min_inclusive", "max_inclusive")?;
                if max_inclusive < min_inclusive {
                    return Err(LoadError::new(format!(
                        "Max must be larger than min: [{min_inclusive}, {max_inclusive}]"
                    )));
                }
                Ok(IntProvider::ClampedNormal {
                    mean,
                    deviation,
                    min_inclusive,
                    max_inclusive,
                })
            }
            "trapezoid" => {
                let (min_inclusive, max_inclusive) = min_max("min", "max")?;
                let plateau = f.required("plateau", parse_int)?;
                if max_inclusive < min_inclusive {
                    return Err(LoadError::new(format!(
                        "Max must be larger than min: [{min_inclusive}, {max_inclusive}]"
                    )));
                }
                // `plateau > max - min` in Java int arithmetic.
                if plateau > max_inclusive.wrapping_sub(min_inclusive) {
                    return Err(LoadError::new(format!(
                        "Plateau can at most be the full span: [{min_inclusive}, {max_inclusive}]"
                    )));
                }
                Ok(IntProvider::Trapezoid {
                    min_inclusive,
                    max_inclusive,
                    plateau,
                })
            }
            other => Err(unknown_type(other)),
        }
    }

    /// `IntProviders.codec(minValue, maxValue)`.
    pub fn from_json_range(
        value: &Value,
        min_value: i32,
        max_value: i32,
    ) -> LoadResult<IntProvider> {
        let provider = IntProvider::from_json(value)?;
        if provider.min_inclusive() < min_value {
            return Err(LoadError::new(format!(
                "Value provider too low: {min_value} [{}-{}]",
                provider.min_inclusive(),
                provider.max_inclusive()
            )));
        }
        if provider.max_inclusive() > max_value {
            return Err(LoadError::new(format!(
                "Value provider too high: {max_value} [{}-{}]",
                provider.min_inclusive(),
                provider.max_inclusive()
            )));
        }
        Ok(provider)
    }

    /// `IntProviders.NON_NEGATIVE_CODEC`.
    pub fn non_negative_from_json(value: &Value) -> LoadResult<IntProvider> {
        IntProvider::from_json_range(value, 0, i32::MAX)
    }

    /// `IntProviders.POSITIVE_CODEC`.
    pub fn positive_from_json(value: &Value) -> LoadResult<IntProvider> {
        IntProvider::from_json_range(value, 1, i32::MAX)
    }
}
