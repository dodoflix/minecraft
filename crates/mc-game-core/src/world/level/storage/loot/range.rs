//! `IntRangePredicate`, `FloatRangePredicate` and `IntLimit`: bounds whose ends are context number providers.

use mc_dfu::{Codec, Either, MapCodec, record};

use super::context::LootContext;
use super::providers::number::{
    FloatHolder, IntHolder, float_exactly, float_holder_codec, int_exactly, int_holder_codec,
};
use super::validation::{ValidationContext, validate_holder, validate_holder_opt};
use mc_core::math::mth;

/// `IntRangePredicate`.
#[derive(Clone)]
pub enum IntRangePredicate {
    Point(IntHolder),
    Line {
        min: Option<IntHolder>,
        max: Option<IntHolder>,
    },
}

impl IntRangePredicate {
    /// `CODEC`: a provider (exact value) or `{min?, max?}`.
    pub fn codec() -> Codec<IntRangePredicate> {
        let line = record!(IntLine {
            min: int_holder_codec().optional_field_of("min"),
            max: int_holder_codec().optional_field_of("max")
        })
        .codec();
        int_holder_codec().either(&line).xmap(
            |e| match e {
                Either::Left(h) => IntRangePredicate::Point(h),
                Either::Right(l) => IntRangePredicate::Line {
                    min: l.min,
                    max: l.max,
                },
            },
            |r: &IntRangePredicate| match r {
                IntRangePredicate::Point(h) => Either::Left(h.clone()),
                IntRangePredicate::Line { min, max } => Either::Right(IntLine {
                    min: min.clone(),
                    max: max.clone(),
                }),
            },
        )
    }

    pub fn exact(value: i32) -> IntRangePredicate {
        IntRangePredicate::Point(int_exactly(value))
    }

    /// `test(context, input)`.
    pub fn test(&self, context: &mut LootContext, input: i32) -> bool {
        match self {
            IntRangePredicate::Point(h) => {
                h.value(context.get_resolver()).get_int(context) == input
            }
            IntRangePredicate::Line { min, max } => {
                if let Some(min) = min {
                    if input < min.value(context.get_resolver()).get_int(context) {
                        return false;
                    }
                }
                if let Some(max) = max {
                    if input > max.value(context.get_resolver()).get_int(context) {
                        return false;
                    }
                }
                true
            }
        }
    }

    /// `test(context, provider)`.
    pub fn test_provider(&self, context: &mut LootContext, input: &IntHolder) -> bool {
        let v = input.value(context.get_resolver()).get_int(context);
        self.test(context, v)
    }

    pub fn validate(&self, context: &ValidationContext) {
        match self {
            IntRangePredicate::Point(h) => validate_holder(context, h),
            IntRangePredicate::Line { min, max } => {
                validate_holder_opt(context, "min", min);
                validate_holder_opt(context, "max", max);
            }
        }
    }
}

#[derive(Clone)]
struct IntLine {
    min: Option<IntHolder>,
    max: Option<IntHolder>,
}

/// `FloatRangePredicate`.
#[derive(Clone)]
pub enum FloatRangePredicate {
    Point(FloatHolder),
    Line {
        min: Option<FloatHolder>,
        max: Option<FloatHolder>,
    },
}

#[derive(Clone)]
struct FloatLine {
    min: Option<FloatHolder>,
    max: Option<FloatHolder>,
}

impl FloatRangePredicate {
    pub fn codec() -> Codec<FloatRangePredicate> {
        let line = record!(FloatLine {
            min: float_holder_codec().optional_field_of("min"),
            max: float_holder_codec().optional_field_of("max"),
        })
        .codec();
        float_holder_codec().either(&line).xmap(
            |e| match e {
                Either::Left(h) => FloatRangePredicate::Point(h),
                Either::Right(l) => FloatRangePredicate::Line {
                    min: l.min,
                    max: l.max,
                },
            },
            |r: &FloatRangePredicate| match r {
                FloatRangePredicate::Point(h) => Either::Left(h.clone()),
                FloatRangePredicate::Line { min, max } => Either::Right(FloatLine {
                    min: min.clone(),
                    max: max.clone(),
                }),
            },
        )
    }

    pub fn exact(value: f32) -> FloatRangePredicate {
        FloatRangePredicate::Point(float_exactly(value))
    }

    pub fn test(&self, context: &mut LootContext, input: f32) -> bool {
        match self {
            FloatRangePredicate::Point(h) => {
                h.value(context.get_resolver()).get_float(context) == input
            }
            FloatRangePredicate::Line { min, max } => {
                if let Some(min) = min {
                    if !(input >= min.value(context.get_resolver()).get_float(context)) {
                        return false;
                    }
                }
                if let Some(max) = max {
                    if !(input <= max.value(context.get_resolver()).get_float(context)) {
                        return false;
                    }
                }
                true
            }
        }
    }

    pub fn test_provider(&self, context: &mut LootContext, input: &FloatHolder) -> bool {
        let v = input.value(context.get_resolver()).get_float(context);
        self.test(context, v)
    }

    pub fn validate(&self, context: &ValidationContext) {
        match self {
            FloatRangePredicate::Point(h) => validate_holder(context, h),
            FloatRangePredicate::Line { min, max } => {
                validate_holder_opt(context, "min", min);
                validate_holder_opt(context, "max", max);
            }
        }
    }
}

/// `IntLimit`.
#[derive(Clone)]
pub struct IntLimit {
    pub min: Option<IntHolder>,
    pub max: Option<IntHolder>,
}

impl IntLimit {
    pub fn codec() -> Codec<IntLimit> {
        Self::map_codec().codec()
    }

    fn map_codec() -> MapCodec<IntLimit> {
        record!(IntLimit {
            min: int_holder_codec().optional_field_of("min"),
            max: int_holder_codec().optional_field_of("max")
        })
    }

    /// `clamp(context, input)`.
    pub fn clamp(&self, context: &mut LootContext, input: i32) -> i32 {
        match (&self.min, &self.max) {
            (None, None) => input,
            (None, Some(max)) => max
                .value(context.get_resolver())
                .get_int(context)
                .min(input),
            (Some(min), None) => min
                .value(context.get_resolver())
                .get_int(context)
                .max(input),
            (Some(min), Some(max)) => {
                let lo = min.value(context.get_resolver()).get_int(context);
                let hi = max.value(context.get_resolver()).get_int(context);
                mth::clamp_i32(input, lo, hi)
            }
        }
    }

    pub fn validate(&self, context: &ValidationContext) {
        validate_holder_opt(context, "min", &self.min);
        validate_holder_opt(context, "max", &self.max);
    }
}
