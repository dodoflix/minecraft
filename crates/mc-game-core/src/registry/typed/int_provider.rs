//! `util.valueproviders.IntProvider(s)`: the codec (`IntProviders.codec(min, max)`) and a value type for the providers
//! gameplay code reads out of registry data (constant and uniform are sampled; the other provider types keep their
//! validated JSON and are not sampled).

use mc_core::random::RandomSource;
use mc_nbt::Tag;
use serde_json::{Map, Value};

use crate::registry::codec::{Failure, RegistryDecodeContext, Schema};
use crate::server::world_dimensions::nbt_to_json;

/// An `IntProvider` read from data.
#[derive(Clone, Debug, PartialEq)]
pub enum IntProvider {
    Constant(i32),
    Uniform {
        min_inclusive: i32,
        max_inclusive: i32,
    },
    /// Any other provider type (`biased_to_bottom`, `clamped`, `weighted_list`, ...): validated JSON and its bounds.
    Other {
        json: Value,
        min_inclusive: i32,
        max_inclusive: i32,
    },
}

impl IntProvider {
    pub fn min_inclusive(&self) -> i32 {
        match self {
            IntProvider::Constant(v) => *v,
            IntProvider::Uniform { min_inclusive, .. }
            | IntProvider::Other { min_inclusive, .. } => *min_inclusive,
        }
    }

    pub fn max_inclusive(&self) -> i32 {
        match self {
            IntProvider::Constant(v) => *v,
            IntProvider::Uniform { max_inclusive, .. }
            | IntProvider::Other { max_inclusive, .. } => *max_inclusive,
        }
    }

    /// `sample(random)` for the provider types modelled here; `None` for `Other`.
    pub fn sample(&self, random: &mut dyn RandomSource) -> Option<i32> {
        match self {
            IntProvider::Constant(v) => Some(*v),
            // `Mth.randomBetweenInclusive(random, min, max)` = `random.nextInt(max - min + 1) + min`
            IntProvider::Uniform {
                min_inclusive,
                max_inclusive,
            } => Some(random.next_int_between_inclusive(*min_inclusive, *max_inclusive)),
            IntProvider::Other { .. } => None,
        }
    }

    /// Typed read of validated provider JSON (see [`schema`]).
    pub fn from_json(json: &Value) -> Option<IntProvider> {
        if let Some(v) = int(json) {
            return Some(IntProvider::Constant(v));
        }
        let o = json.as_object()?;
        let (min, max) = bounds(o)?;
        let ty = o.get("type")?.as_str()?;
        match ty.strip_prefix("minecraft:").unwrap_or(ty) {
            "constant" => Some(IntProvider::Constant(int(o.get("value")?)?)),
            "uniform" => Some(IntProvider::Uniform {
                min_inclusive: min,
                max_inclusive: max,
            }),
            _ => Some(IntProvider::Other {
                json: json.clone(),
                min_inclusive: min,
                max_inclusive: max,
            }),
        }
    }
}

fn int(v: &Value) -> Option<i32> {
    v.as_i64()
        .map(|i| i as i32)
        .or_else(|| v.as_f64().map(|f| f as i32))
}

/// `(minInclusive, maxInclusive)` of a provider object.
fn bounds(o: &Map<String, Value>) -> Option<(i32, i32)> {
    let ty = o.get("type")?.as_str()?;
    let field = |name: &str| o.get(name).and_then(int);
    match ty.strip_prefix("minecraft:").unwrap_or(ty) {
        "constant" => field("value").map(|v| (v, v)),
        "uniform"
        | "biased_to_bottom"
        | "very_biased_to_bottom"
        | "trapezoid"
        | "clamped_normal" => Some((field("min_inclusive")?, field("max_inclusive")?)),
        // `ClampedInt.minInclusive()` / `maxInclusive()` narrow the bounds to the source's
        "clamped" => {
            let source = IntProvider::from_json(o.get("source")?)?;
            Some((
                field("min_inclusive")?.max(source.min_inclusive()),
                field("max_inclusive")?.min(source.max_inclusive()),
            ))
        }
        "weighted_list" => {
            let list = o.get("distribution")?.as_array()?;
            let mut min = i32::MAX;
            let mut max = i32::MIN;
            for entry in list {
                let p = IntProvider::from_json(entry.get("data")?)?;
                min = min.min(p.min_inclusive());
                max = max.max(p.max_inclusive());
            }
            Some((min, max))
        }
        _ => None,
    }
}

fn provider_types(ty: &str) -> Option<Schema> {
    use crate::registry::codec::req;
    Some(match ty.strip_prefix("minecraft:").unwrap_or(ty) {
        "constant" => Schema::Record(vec![req("value", Schema::Int)]),
        "uniform" | "biased_to_bottom" | "very_biased_to_bottom" => Schema::custom(|v, ctx| {
            let t = Schema::Record(vec![
                req("min_inclusive", Schema::Int),
                req("max_inclusive", Schema::Int),
            ])
            .convert(v, ctx)?;
            let o = v.as_object().ok_or("Not a map")?;
            let (min, max) = (
                int(&o["min_inclusive"]).unwrap_or(0),
                int(&o["max_inclusive"]).unwrap_or(0),
            );
            if max < min {
                Err(format!(
                    "Max must be at least min, min_inclusive: {min}, max_inclusive: {max}"
                ))
            } else {
                Ok(t)
            }
        }),
        "trapezoid" => Schema::custom(|v, ctx| {
            let t = Schema::Record(vec![
                req("min_inclusive", Schema::Int),
                req("max_inclusive", Schema::Int),
                crate::registry::codec::default("plateau", Schema::Int, Value::from(0)),
            ])
            .convert(v, ctx)?;
            let o = v.as_object().ok_or("Not a map")?;
            let (min, max) = (
                int(&o["min_inclusive"]).unwrap_or(0),
                int(&o["max_inclusive"]).unwrap_or(0),
            );
            if max < min {
                Err(format!(
                    "Max must be at least min, min_inclusive: {min}, max_inclusive: {max}"
                ))
            } else {
                Ok(t)
            }
        }),
        "clamped" => Schema::Record(vec![
            req("source", schema(i32::MIN, i32::MAX)),
            req("min_inclusive", Schema::Int),
            req("max_inclusive", Schema::Int),
        ]),
        "clamped_normal" => Schema::Record(vec![
            req("mean", Schema::Float),
            req("deviation", Schema::Float),
            req("min_inclusive", Schema::Int),
            req("max_inclusive", Schema::Int),
        ]),
        "weighted_list" => Schema::Record(vec![req(
            "distribution",
            super::state_provider::non_empty_weighted_list(schema(i32::MIN, i32::MAX)),
        )]),
        _ => return None,
    })
}

/// `IntProviders.validate(minValue, maxValue, provider)`.
fn check_range(provider: &IntProvider, min_value: i32, max_value: i32) -> Result<(), String> {
    let (min, max) = (provider.min_inclusive(), provider.max_inclusive());
    if min < min_value {
        Err(format!("Value provider too low: {min_value} [{min}-{max}]"))
    } else if max > max_value {
        Err(format!(
            "Value provider too high: {max_value} [{min}-{max}]"
        ))
    } else {
        Ok(())
    }
}

/// `IntProviders.codec(minValue, maxValue)`: a constant (bare number) or a typed provider whose bounds lie in the range.
pub fn schema(min_value: i32, max_value: i32) -> Schema {
    Schema::custom_partial(move |v, ctx: &RegistryDecodeContext| {
        let tag = if v.is_number() {
            Schema::Int.convert(v, ctx)?
        } else {
            // `Codec.either(Codec.INT, dispatch)`: a failure with a partial result is reported alone
            let second = match v {
                Value::Object(_) => Schema::Dispatch {
                    key: "type",
                    resolve: provider_types,
                }
                .decode(v, ctx),
                _ => Err(Failure::from(format!("Not a JSON object: {v}"))),
            };
            let body = match second {
                Ok(body) => body,
                Err(failure) if failure.is_thrown() => return Err(failure),
                // `validate` is a `flatMap`: it checks the partial provider and joins its message to the failure's
                Err(failure) if failure.partial.is_some() => {
                    let partial = failure.partial.as_ref().map(nbt_to_json);
                    return Err(match partial.as_ref().and_then(IntProvider::from_json) {
                        Some(provider) => match check_range(&provider, min_value, max_value) {
                            Ok(()) => failure,
                            Err(range) => Failure::from(format!("{}; {range}", failure.message)),
                        },
                        None => failure,
                    });
                }
                Err(failure) => {
                    return Err(Failure::from(format!(
                        "Failed to parse either. First: Not a number: {v}; Second: {}",
                        failure.message
                    )));
                }
            };
            // a `constant` object re-encodes as the bare number
            match (&body, v.get("type").and_then(Value::as_str)) {
                (Tag::Compound(c), Some(t))
                    if t.strip_prefix("minecraft:").unwrap_or(t) == "constant" =>
                {
                    c.get("value").cloned().unwrap_or(body.clone())
                }
                _ => body,
            }
        };
        let provider = IntProvider::from_json(v)
            .ok_or_else(|| Failure::from("Unsupported value provider".to_owned()))?;
        check_range(&provider, min_value, max_value).map_err(Failure::from)?;
        Ok(tag)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::codec::{Lookup, RegistryLookup};
    use mc_core::ident::Identifier;
    use serde_json::json;

    struct L;
    impl RegistryLookup for L {
        fn contains(&self, _: &str, _: &Identifier) -> Lookup {
            Lookup::Yes
        }
        fn tag_exists(&self, _: &str, _: &Identifier) -> Lookup {
            Lookup::Yes
        }
    }

    fn conv(min: i32, max: i32, v: Value) -> Result<Tag, String> {
        let l = L;
        schema(min, max).convert(&v, &RegistryDecodeContext::new(&l))
    }

    #[test]
    fn providers() {
        assert_eq!(conv(0, 15, json!(7)).unwrap(), Tag::Int(7));
        assert_eq!(
            conv(0, 15, json!({"type": "minecraft:constant", "value": 3})).unwrap(),
            Tag::Int(3)
        );
        assert!(
            matches!(conv(0, 15, json!({"type": "minecraft:uniform", "min_inclusive": 0, "max_inclusive": 7})).unwrap(), Tag::Compound(c) if c.len() == 3)
        );
        assert_eq!(
            conv(0, 15, json!(16)).unwrap_err(),
            "Value provider too high: 15 [16-16]"
        );
        assert_eq!(
            conv(
                0,
                15,
                json!({"type": "uniform", "min_inclusive": -1, "max_inclusive": 3})
            )
            .unwrap_err(),
            "Value provider too low: 0 [-1-3]"
        );
        assert!(
            conv(
                0,
                15,
                json!({"type": "uniform", "min_inclusive": 5, "max_inclusive": 3})
            )
            .unwrap_err()
            .contains("Second: Max must be at least min")
        );
    }

    #[test]
    fn the_range_check_sees_a_partial_provider() {
        let weighted =
            |entries: Value| json!({"type": "minecraft:weighted_list", "distribution": entries});
        let failed = json!({"data": "x", "weight": 1});
        let too_high = conv(0, 15, weighted(json!([{"data": 20, "weight": 1}, failed])));
        assert!(
            too_high
                .unwrap_err()
                .ends_with("Not a JSON object: \"x\"; Value provider too high: 15 [20-20]")
        );
        let in_range = conv(0, 15, weighted(json!([{"data": 5, "weight": 1}, failed])));
        assert!(!in_range.unwrap_err().contains("Value provider"));
        // a clamped provider is narrowed to its source
        let clamped = json!({"type": "minecraft:clamped", "min_inclusive": 0, "max_inclusive": 99, "source": 30});
        assert_eq!(
            conv(0, 15, clamped).unwrap_err(),
            "Value provider too high: 15 [30-30]"
        );
    }
}
