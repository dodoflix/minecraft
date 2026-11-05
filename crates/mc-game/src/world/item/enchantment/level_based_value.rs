//! `LevelBasedValue`: a float that depends on the enchantment level.

use std::sync::LazyLock;

use mc_core::math::java::java_float_to_string;
use mc_core::math::mth;
use mc_dfu::{Codec, DataResult, Either, FLOAT, MapCodec, record};

use super::codecs::type_codec;

const REGISTRY: &str = "minecraft:enchantment_level_based_value_type";

#[derive(Clone, Debug, PartialEq)]
pub enum LevelBasedValue {
    Constant(f32),
    Clamped { value: Box<LevelBasedValue>, min: f32, max: f32 },
    Exponent { base: Box<LevelBasedValue>, power: Box<LevelBasedValue> },
    Fraction { numerator: Box<LevelBasedValue>, denominator: Box<LevelBasedValue> },
    LevelsSquared { added: f32 },
    Linear { base: f32, per_level_above_first: f32 },
    Lookup { values: Vec<f32>, fallback: Box<LevelBasedValue> },
}

impl LevelBasedValue {
    /// `LevelBasedValue.constant(value)`.
    pub fn constant(value: f32) -> LevelBasedValue {
        LevelBasedValue::Constant(value)
    }

    /// `LevelBasedValue.perLevel(base, perLevelAboveFirst)`.
    pub fn per_level(base: f32, per_level_above_first: f32) -> LevelBasedValue {
        LevelBasedValue::Linear { base, per_level_above_first }
    }

    /// `calculate(level)`.
    pub fn calculate(&self, level: i32) -> f32 {
        match self {
            LevelBasedValue::Constant(value) => *value,
            LevelBasedValue::Clamped { value, min, max } => mth::clamp_f32(value.calculate(level), *min, *max),
            LevelBasedValue::Exponent { base, power } => (base.calculate(level) as f64).powf(power.calculate(level) as f64) as f32,
            LevelBasedValue::Fraction { numerator, denominator } => {
                let denominator = denominator.calculate(level);
                if denominator == 0.0 { 0.0 } else { numerator.calculate(level) / denominator }
            }
            LevelBasedValue::LevelsSquared { added } => level.wrapping_mul(level) as f32 + added,
            LevelBasedValue::Linear { base, per_level_above_first } => base + per_level_above_first * level.wrapping_sub(1) as f32,
            LevelBasedValue::Lookup { values, fallback } => {
                // `level <= values.size() ? values.get(level - 1)`: levels are >= 1 (`ItemEnchantments` levels), a lower
                // level would make `List.get` throw in vanilla
                if level >= 1 && level as usize <= values.len() { values[level as usize - 1] } else { fallback.calculate(level) }
            }
        }
    }

    /// `LevelBasedValue.CODEC`: a bare float is a `Constant`, anything else is dispatched on `type`.
    pub fn codec() -> Codec<LevelBasedValue> {
        static CODEC: LazyLock<Codec<LevelBasedValue>> = LazyLock::new(build);
        CODEC.clone()
    }
}

fn boxed() -> Codec<Box<LevelBasedValue>> {
    Codec::lazy(LevelBasedValue::codec).xmap(Box::new, |b: &Box<LevelBasedValue>| (**b).clone())
}

#[derive(Clone)]
struct ClampedRec {
    value: Box<LevelBasedValue>,
    min: f32,
    max: f32,
}
struct ExponentRec {
    base: Box<LevelBasedValue>,
    power: Box<LevelBasedValue>,
}
struct FractionRec {
    numerator: Box<LevelBasedValue>,
    denominator: Box<LevelBasedValue>,
}
struct LevelsSquaredRec {
    added: f32,
}
struct LinearRec {
    base: f32,
    per_level_above_first: f32,
}
struct LookupRec {
    values: Vec<f32>,
    fallback: Box<LevelBasedValue>,
}

fn arm(ty: &str) -> MapCodec<LevelBasedValue> {
    use LevelBasedValue as L;
    match ty {
        "minecraft:clamped" => record!(ClampedRec {
            value: boxed().field_of("value"),
            min: FLOAT.field_of("min"),
            max: FLOAT.field_of("max"),
        })
        .validate(|u: ClampedRec| {
            if u.max <= u.min {
                let m = format!("Max must be larger than min, min: {}, max: {}", java_float_to_string(u.min), java_float_to_string(u.max));
                DataResult::error(move || m.clone())
            } else {
                DataResult::success(u)
            }
        })
        .xmap(
            |r| L::Clamped { value: r.value, min: r.min, max: r.max },
            |v: &L| match v {
                L::Clamped { value, min, max } => ClampedRec { value: value.clone(), min: *min, max: *max },
                _ => unreachable!("dispatched by type"),
            },
        ),
        "minecraft:exponent" => record!(ExponentRec { base: boxed().field_of("base"), power: boxed().field_of("power") }).xmap(
            |r| L::Exponent { base: r.base, power: r.power },
            |v: &L| match v {
                L::Exponent { base, power } => ExponentRec { base: base.clone(), power: power.clone() },
                _ => unreachable!("dispatched by type"),
            },
        ),
        "minecraft:fraction" => {
            record!(FractionRec { numerator: boxed().field_of("numerator"), denominator: boxed().field_of("denominator") }).xmap(
                |r| L::Fraction { numerator: r.numerator, denominator: r.denominator },
                |v: &L| match v {
                    L::Fraction { numerator, denominator } => FractionRec { numerator: numerator.clone(), denominator: denominator.clone() },
                    _ => unreachable!("dispatched by type"),
                },
            )
        }
        "minecraft:levels_squared" => record!(LevelsSquaredRec { added: FLOAT.field_of("added") }).xmap(
            |r| L::LevelsSquared { added: r.added },
            |v: &L| match v {
                L::LevelsSquared { added } => LevelsSquaredRec { added: *added },
                _ => unreachable!("dispatched by type"),
            },
        ),
        "minecraft:linear" => record!(LinearRec {
            base: FLOAT.field_of("base"),
            per_level_above_first: FLOAT.field_of("per_level_above_first"),
        })
        .xmap(
            |r| L::Linear { base: r.base, per_level_above_first: r.per_level_above_first },
            |v: &L| match v {
                L::Linear { base, per_level_above_first } => LinearRec { base: *base, per_level_above_first: *per_level_above_first },
                _ => unreachable!("dispatched by type"),
            },
        ),
        "minecraft:lookup" => record!(LookupRec { values: FLOAT.list_of().field_of("values"), fallback: boxed().field_of("fallback") }).xmap(
            |r| L::Lookup { values: r.values, fallback: r.fallback },
            |v: &L| match v {
                L::Lookup { values, fallback } => LookupRec { values: values.clone(), fallback: fallback.clone() },
                _ => unreachable!("dispatched by type"),
            },
        ),
        _ => unreachable!("type_codec only yields registered names"),
    }
}

fn build() -> Codec<LevelBasedValue> {
    let constant = FLOAT.xmap(LevelBasedValue::Constant, |v: &LevelBasedValue| match v {
        LevelBasedValue::Constant(c) => *c,
        _ => unreachable!("encoded through the dispatch codec"),
    });
    let dispatch = type_codec(REGISTRY).dispatch(
        "type",
        |v: &LevelBasedValue| match v {
            LevelBasedValue::Clamped { .. } => "minecraft:clamped",
            LevelBasedValue::Exponent { .. } => "minecraft:exponent",
            LevelBasedValue::Fraction { .. } => "minecraft:fraction",
            LevelBasedValue::LevelsSquared { .. } => "minecraft:levels_squared",
            LevelBasedValue::Linear { .. } => "minecraft:linear",
            LevelBasedValue::Lookup { .. } => "minecraft:lookup",
            // constants never reach the dispatch branch (`Either.left`)
            LevelBasedValue::Constant(_) => "minecraft:linear",
        },
        |ty: &&'static str| arm(ty),
    );
    constant.either(&dispatch).xmap(
        |e| match e {
            Either::Left(v) | Either::Right(v) => v,
        },
        |v: &LevelBasedValue| match v {
            LevelBasedValue::Constant(_) => Either::Left(v.clone()),
            _ => Either::Right(v.clone()),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculate_matches_vanilla_formulas() {
        let linear = LevelBasedValue::per_level(0.5, 0.5);
        assert_eq!(linear.calculate(1), 0.5);
        assert_eq!(linear.calculate(3), 1.5);
        assert_eq!(LevelBasedValue::LevelsSquared { added: 6.0 }.calculate(3), 15.0);
        let lookup = LevelBasedValue::Lookup { values: vec![1.0, 2.0], fallback: Box::new(LevelBasedValue::Constant(9.0)) };
        assert_eq!((lookup.calculate(2), lookup.calculate(3)), (2.0, 9.0));
        let frac = LevelBasedValue::Fraction { numerator: Box::new(LevelBasedValue::Constant(1.0)), denominator: Box::new(LevelBasedValue::Constant(0.0)) };
        assert_eq!(frac.calculate(1), 0.0);
    }
}
