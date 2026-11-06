//! `net.minecraft.util.valueproviders.{FloatProvider, FloatProviders, ConstantFloat, UniformFloat,
//! ClampedNormalFloat, TrapezoidFloat, MultipliedFloats}`.

use serde_json::Value;

use mc_core::math::mth;
use mc_core::random::RandomSource;

use super::{dispatch_type, unknown_type};
use crate::data::LoadError;
use crate::densityfunction::codec::{LoadResult, parse_f32};

#[derive(Clone, Debug, PartialEq)]
pub enum FloatProvider {
    Constant(f32),
    Uniform {
        min: f32,
        max: f32,
    },
    ClampedNormal {
        mean: f32,
        deviation: f32,
        min: f32,
        max: f32,
    },
    Trapezoid {
        min: f32,
        max: f32,
        plateau: f32,
    },
}

impl FloatProvider {
    /// `ConstantFloat.of(value)` (`-0.0` collapses to the shared `ZERO`).
    pub fn constant(value: f32) -> FloatProvider {
        FloatProvider::Constant(if value == 0.0 { 0.0 } else { value })
    }

    /// `ClampedNormalFloat.sample(random, mean, deviation, min, max)`.
    pub fn sample_clamped_normal(
        random: &mut dyn RandomSource,
        mean: f32,
        deviation: f32,
        min: f32,
        max: f32,
    ) -> f32 {
        mth::clamp_f32(mth::normal(random, mean, deviation), min, max)
    }

    pub fn sample(&self, random: &mut dyn RandomSource) -> f32 {
        match self {
            FloatProvider::Constant(value) => *value,
            FloatProvider::Uniform { min, max } => mth::random_between(random, *min, *max),
            FloatProvider::ClampedNormal {
                mean,
                deviation,
                min,
                max,
            } => FloatProvider::sample_clamped_normal(random, *mean, *deviation, *min, *max),
            FloatProvider::Trapezoid { min, max, plateau } => {
                let range = max - min;
                let plateau_start = (range - plateau) / 2.0;
                let plateau_end = range - plateau_start;
                min + random.next_float() * plateau_end + random.next_float() * plateau_start
            }
        }
    }

    pub fn min(&self) -> f32 {
        match self {
            FloatProvider::Constant(value) => *value,
            FloatProvider::Uniform { min, .. }
            | FloatProvider::ClampedNormal { min, .. }
            | FloatProvider::Trapezoid { min, .. } => *min,
        }
    }

    pub fn max(&self) -> f32 {
        match self {
            FloatProvider::Constant(value) => *value,
            FloatProvider::Uniform { max, .. }
            | FloatProvider::ClampedNormal { max, .. }
            | FloatProvider::Trapezoid { max, .. } => *max,
        }
    }

    /// `FloatProviders.CODEC`: a number is a constant, an object is dispatched on its `type`.
    pub fn from_json(value: &Value) -> LoadResult<FloatProvider> {
        if let Value::Number(_) = value {
            return Ok(FloatProvider::constant(parse_f32(value)?));
        }
        let (ty, f) = dispatch_type(value).map_err(|e| {
            if value.is_object() {
                e
            } else {
                LoadError::new("Failed to parse either: expected number or object")
            }
        })?;
        let bounds = |min: f32, max: f32| -> LoadResult<()> {
            if max < min {
                return Err(LoadError::new(format!(
                    "Max must be larger than min: [{min:?}, {max:?}]"
                )));
            }
            Ok(())
        };
        match ty.as_str() {
            "constant" => Ok(FloatProvider::constant(f.required("value", parse_f32)?)),
            "uniform" => {
                let min = f.required("min_inclusive", parse_f32)?;
                let max = f.required("max_exclusive", parse_f32)?;
                if max <= min {
                    return Err(LoadError::new(format!(
                        "Max must be larger than min, min: {min:?}, max: {max:?}"
                    )));
                }
                Ok(FloatProvider::Uniform { min, max })
            }
            "clamped_normal" => {
                let mean = f.required("mean", parse_f32)?;
                let deviation = f.required("deviation", parse_f32)?;
                let min = f.required("min", parse_f32)?;
                let max = f.required("max", parse_f32)?;
                bounds(min, max)?;
                Ok(FloatProvider::ClampedNormal {
                    mean,
                    deviation,
                    min,
                    max,
                })
            }
            "trapezoid" => {
                let min = f.required("min", parse_f32)?;
                let max = f.required("max", parse_f32)?;
                let plateau = f.required("plateau", parse_f32)?;
                bounds(min, max)?;
                if plateau > max - min {
                    return Err(LoadError::new(format!(
                        "Plateau can at most be the full span: [{min:?}, {max:?}]"
                    )));
                }
                Ok(FloatProvider::Trapezoid { min, max, plateau })
            }
            other => Err(unknown_type(other)),
        }
    }

    /// `FloatProviders.codec(minValue, maxValue)`.
    pub fn from_json_range(
        value: &Value,
        min_value: f32,
        max_value: f32,
    ) -> LoadResult<FloatProvider> {
        let provider = FloatProvider::from_json_min(value, min_value)?;
        if provider.max() > max_value {
            return Err(LoadError::new(format!(
                "Value provider too high: {max_value:?} [{:?}-{:?}]",
                provider.min(),
                provider.max()
            )));
        }
        Ok(provider)
    }

    /// `FloatProviders.codec(minValue)`.
    pub fn from_json_min(value: &Value, min_value: f32) -> LoadResult<FloatProvider> {
        let provider = FloatProvider::from_json(value)?;
        if provider.min() < min_value {
            return Err(LoadError::new(format!(
                "Value provider too low: {min_value:?} [{:?}-{:?}]",
                provider.min(),
                provider.max()
            )));
        }
        Ok(provider)
    }
}

/// `MultipliedFloats.sample`: the product of the factors' samples, in order.
pub fn sample_multiplied(random: &mut dyn RandomSource, values: &[FloatProvider]) -> f32 {
    let mut result = 1.0f32;
    for value in values {
        result *= value.sample(random);
    }
    result
}
