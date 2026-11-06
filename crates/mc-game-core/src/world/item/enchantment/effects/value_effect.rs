//! `EnchantmentValueEffect` (`add`, `all_of`, `multiply`, `remove_binomial`, `exponential`, `set`): float -> float
//! transformations with the enchantment level.

use std::sync::LazyLock;

use mc_core::math::java::round_f64;
use mc_core::random::RandomSource;
use mc_dfu::{Codec, MapCodec, record};

use crate::world::item::enchantment::codecs::type_codec;
use crate::world::item::enchantment::level_based_value::LevelBasedValue;

const REGISTRY: &str = "minecraft:enchantment_value_effect_type";

#[derive(Clone, Debug, PartialEq)]
pub enum EnchantmentValueEffect {
    Add {
        value: LevelBasedValue,
    },
    AllOf {
        effects: Vec<EnchantmentValueEffect>,
    },
    Multiply {
        factor: LevelBasedValue,
    },
    RemoveBinomial {
        chance: LevelBasedValue,
    },
    Exponential {
        base: LevelBasedValue,
        exponent: LevelBasedValue,
    },
    Set {
        value: LevelBasedValue,
    },
}

impl EnchantmentValueEffect {
    /// `process(enchantmentLevel, random, inputValue)`.
    pub fn process(
        &self,
        enchantment_level: i32,
        random: &mut dyn RandomSource,
        input: f32,
    ) -> f32 {
        match self {
            EnchantmentValueEffect::Add { value } => input + value.calculate(enchantment_level),
            EnchantmentValueEffect::AllOf { effects } => {
                let mut value = input;
                for effect in effects {
                    value = effect.process(enchantment_level, random, value);
                }
                value
            }
            EnchantmentValueEffect::Multiply { factor } => {
                input * factor.calculate(enchantment_level)
            }
            EnchantmentValueEffect::RemoveBinomial { chance } => {
                remove_binomial(chance.calculate(enchantment_level), random, input)
            }
            EnchantmentValueEffect::Exponential { base, exponent } => {
                (input as f64
                    * (base.calculate(enchantment_level) as f64)
                        .powf(exponent.calculate(enchantment_level) as f64)) as f32
            }
            EnchantmentValueEffect::Set { value } => value.calculate(enchantment_level),
        }
    }

    pub fn codec() -> Codec<EnchantmentValueEffect> {
        static CODEC: LazyLock<Codec<EnchantmentValueEffect>> = LazyLock::new(build);
        CODEC.clone()
    }
}

/// `RemoveBinomial.process`.
fn remove_binomial(p: f32, random: &mut dyn RandomSource, n: f32) -> f32 {
    let mut drop = 0i32;
    if !(n <= 128.0) && !(n * p < 20.0) && !(n * (1.0 - p) < 20.0) {
        let miu = (n * p).floor() as f64;
        let sigma = ((n * p * (1.0 - p)) as f64).sqrt();
        drop = round_f64(miu + random.next_gaussian() * sigma) as i32;
        // `Math.clamp((long) drop, 0, (int) n)`
        drop = (drop as i64).clamp(0, n as i32 as i64) as i32;
    } else {
        let mut y = 0;
        while (y as f32) < n {
            if random.next_float() < p {
                drop += 1;
            }
            y += 1;
        }
    }
    n - drop as f32
}

// `record!` needs named fields: tiny adapters with the vanilla field names
struct ValueRec {
    value: LevelBasedValue,
}
struct FactorRec {
    factor: LevelBasedValue,
}
struct ChanceRec {
    chance: LevelBasedValue,
}
struct ExpRec {
    base: LevelBasedValue,
    exponent: LevelBasedValue,
}
struct EffectsRec {
    effects: Vec<EnchantmentValueEffect>,
}

fn arm(ty: &str) -> MapCodec<EnchantmentValueEffect> {
    use EnchantmentValueEffect as E;
    match ty {
        "minecraft:add" => record!(ValueRec {
            value: LevelBasedValue::codec().field_of("value")
        })
        .xmap(
            |r| E::Add { value: r.value },
            |e: &E| match e {
                E::Add { value } => ValueRec {
                    value: value.clone(),
                },
                _ => unreachable!("dispatched by type"),
            },
        ),
        "minecraft:set" => record!(ValueRec {
            value: LevelBasedValue::codec().field_of("value")
        })
        .xmap(
            |r| E::Set { value: r.value },
            |e: &E| match e {
                E::Set { value } => ValueRec {
                    value: value.clone(),
                },
                _ => unreachable!("dispatched by type"),
            },
        ),
        "minecraft:multiply" => record!(FactorRec {
            factor: LevelBasedValue::codec().field_of("factor")
        })
        .xmap(
            |r| E::Multiply { factor: r.factor },
            |e: &E| match e {
                E::Multiply { factor } => FactorRec {
                    factor: factor.clone(),
                },
                _ => unreachable!("dispatched by type"),
            },
        ),
        "minecraft:remove_binomial" => record!(ChanceRec {
            chance: LevelBasedValue::codec().field_of("chance")
        })
        .xmap(
            |r| E::RemoveBinomial { chance: r.chance },
            |e: &E| match e {
                E::RemoveBinomial { chance } => ChanceRec {
                    chance: chance.clone(),
                },
                _ => unreachable!("dispatched by type"),
            },
        ),
        "minecraft:exponential" => record!(ExpRec {
            base: LevelBasedValue::codec().field_of("base"),
            exponent: LevelBasedValue::codec().field_of("exponent"),
        })
        .xmap(
            |r| E::Exponential {
                base: r.base,
                exponent: r.exponent,
            },
            |e: &E| match e {
                E::Exponential { base, exponent } => ExpRec {
                    base: base.clone(),
                    exponent: exponent.clone(),
                },
                _ => unreachable!("dispatched by type"),
            },
        ),
        "minecraft:all_of" => record!(EffectsRec {
            effects: Codec::lazy(EnchantmentValueEffect::codec)
                .list_of()
                .field_of("effects")
        })
        .xmap(
            |r| E::AllOf { effects: r.effects },
            |e: &E| match e {
                E::AllOf { effects } => EffectsRec {
                    effects: effects.clone(),
                },
                _ => unreachable!("dispatched by type"),
            },
        ),
        _ => unreachable!("type_codec only yields registered names"),
    }
}

fn build() -> Codec<EnchantmentValueEffect> {
    type_codec(REGISTRY).dispatch(
        "type",
        |e: &EnchantmentValueEffect| match e {
            EnchantmentValueEffect::Add { .. } => "minecraft:add",
            EnchantmentValueEffect::AllOf { .. } => "minecraft:all_of",
            EnchantmentValueEffect::Multiply { .. } => "minecraft:multiply",
            EnchantmentValueEffect::RemoveBinomial { .. } => "minecraft:remove_binomial",
            EnchantmentValueEffect::Exponential { .. } => "minecraft:exponential",
            EnchantmentValueEffect::Set { .. } => "minecraft:set",
        },
        |ty: &&'static str| arm(ty),
    )
}
