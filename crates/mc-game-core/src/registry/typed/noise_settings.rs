//! `NoiseGeneratorSettings.DIRECT_CODEC` as a [`Schema`]: an inline noise settings of a level stem validates like the codec
//! does and encodes to the NBT the server saves (`world_gen_settings.dat`): the density function tree with its float
//! constants, the material rules, the vertical anchors, defaults omitted and `Codec.INT` / `FLOAT` / `DOUBLE` tag types.
//! The typed settings the chunk generator uses are decoded separately by `mc_worldgen::noise_settings`.

use mc_core::math::java::java_float_to_string;
use mc_nbt::{ListTag, Tag};
use mc_worldgen::biome::climate::{quantize_coord, unquantize_coord};
use mc_worldgen::noise_settings::{DIMENSION_MAX_Y, DIMENSION_MIN_Y, DIMENSION_Y_SIZE};
use serde_json::Value;

use super::state_provider::noise_parameters;
use crate::registry::codec::{
    Failure, RegistryDecodeContext, Schema, default, interval_codec, opt, req, validate,
};
use crate::registry::keys;
use crate::world::item::enchantment::value_codecs::{
    block_state_schema, vec3i_schema, vertical_anchor_schema,
};

/// `NoiseGeneratorSettings.DIRECT_CODEC` `parse` + `encodeStart`.
pub fn direct(v: &Value, ctx: &RegistryDecodeContext) -> Result<Tag, Failure> {
    settings().decode(v, ctx)
}

fn settings() -> Schema {
    let function_entry = Schema::Record(vec![
        req("label", Schema::String),
        req("function", density_function()),
    ]);
    let aquifers = Schema::Record(
        [
            "barrier",
            "fluid_level_floodedness",
            "fluid_level_spread",
            "lava",
            "exclusion",
            "surface_level",
        ]
        .map(|name| req(name, density_function()))
        .to_vec(),
    );
    let router = Schema::Record(
        [
            "temperature",
            "vegetation",
            "continents",
            "erosion",
            "depth",
            "ridges",
            "chunk_surface_level",
            "final_density",
        ]
        .map(|name| req(name, density_function()))
        .to_vec(),
    );
    Schema::Record(vec![
        req("noise", noise_settings()),
        req("default_block", block_state_schema()),
        req("default_fluid", block_state_schema()),
        req("noise_router", router),
        req("material_rule", material_rule()),
        req(
            "spawn_target",
            Schema::list_of(Schema::map_of(
                Schema::Holder(keys::DENSITY_FUNCTION),
                climate_parameter(),
            )),
        ),
        req("sea_level", Schema::Int),
        req("disable_mob_generation", Schema::Bool),
        opt("aquifers", aquifers),
        req("legacy_random_source", Schema::Bool),
        default(
            "debug_functions",
            Schema::list_of(function_entry),
            Value::Array(Vec::new()),
        ),
    ])
}

/// `NoiseSettings.CODEC`: ranges and `guardY`.
fn noise_settings() -> Schema {
    let record = Schema::Record(vec![
        req("min_y", Schema::IntRange(DIMENSION_MIN_Y, DIMENSION_MAX_Y)),
        req("height", Schema::IntRange(0, DIMENSION_Y_SIZE)),
    ]);
    Schema::custom_partial(move |v, ctx| {
        validate(record.decode(v, ctx), |tag| {
            let Tag::Compound(c) = tag else { return Ok(()) };
            let (min_y, height) = (
                c.get_int("min_y").unwrap_or(0),
                c.get_int("height").unwrap_or(0),
            );
            if min_y + height > DIMENSION_MAX_Y + 1 {
                Err(format!(
                    "min_y + height cannot be higher than: {}",
                    DIMENSION_MAX_Y + 1
                ))
            } else if height % 16 != 0 {
                Err("height has to be a multiple of 16".to_owned())
            } else if min_y % 16 != 0 {
                Err("min_y has to be a multiple of 16".to_owned())
            } else {
                Ok(())
            }
        })
    })
}

/// `Climate.Parameter.CODEC`: a quantized interval written as `[min, max]` (a single number when both ends are equal).
fn climate_parameter() -> Schema {
    interval_codec(Schema::FloatRange(-2.0, 2.0), "min", "max", |min, max| {
        let (Tag::Float(min), Tag::Float(max)) = (min, max) else {
            return Err("Not a number".to_owned());
        };
        if min > max {
            return Err(format!(
                "Cannon construct interval, min > max ({} > {})",
                java_float_to_string(*min),
                java_float_to_string(*max)
            ));
        }
        let (min, max) = (
            unquantize_coord(quantize_coord(*min)),
            unquantize_coord(quantize_coord(*max)),
        );
        Ok(if min == max {
            Tag::Float(min)
        } else {
            Tag::List(
                [Tag::Float(min), Tag::Float(max)]
                    .into_iter()
                    .collect::<ListTag>(),
            )
        })
    })
}

/// `DensityFunctions.NOISE_VALUE_CODEC`.
fn noise_value() -> Schema {
    Schema::FloatRange(-1_000_000.0, 1_000_000.0)
}

/// `ExtraCodecs.POSITIVE_INT`.
pub(crate) fn positive_int() -> Schema {
    Schema::custom(|v, ctx| match Schema::Int.convert(v, ctx)? {
        Tag::Int(n) if n < 1 => Err(format!("Value must be positive: {n}")),
        other => Ok(other),
    })
}

// ---- density functions -----------------------------------------------------------------------------------------

/// `DensityFunction.CODEC`: a reference to a registered function or an inline one.
fn density_function() -> Schema {
    Schema::HolderOrDirect {
        registry: keys::DENSITY_FUNCTION,
        direct: density_direct,
    }
}

/// `DensityFunctions.DIRECT_CODEC`: a number or a `type`-dispatched object; a `constant` encodes as its bare number.
fn density_direct(v: &Value, ctx: &RegistryDecodeContext) -> Result<Tag, Failure> {
    let constant_as_number = |tag: Tag| match &tag {
        Tag::Compound(c) if matches!(c.get("type"), Some(Tag::String(t)) if t == "minecraft:constant") => {
            c.get("value").cloned().unwrap_or(tag)
        }
        _ => tag,
    };
    Schema::either(
        noise_value(),
        Schema::dispatch("worldgen/density_function_type", density_type),
    )
    .decode(v, ctx)
    .map(constant_as_number)
    .map_err(|failure| Failure {
        partial: failure.partial.map(constant_as_number),
        ..failure
    })
}

/// `NormalNoise.CODEC`: a reference to a registered noise or inline parameters.
fn noise_holder() -> Schema {
    Schema::HolderOrDirect {
        registry: keys::NOISE,
        direct: |v, ctx| noise_parameters().decode(v, ctx),
    }
}

fn density_type(name: &str) -> Option<Schema> {
    use Schema as S;
    let df = density_function;
    let input = || S::Record(vec![req("input", df())]);
    let zero = || Value::from(0.0);
    Some(match name.strip_prefix("minecraft:")? {
        "constant" => S::Record(vec![req("value", noise_value())]),
        "blend_alpha" | "blend_offset" | "beardifier" | "end_outer_islands" => S::Record(vec![]),
        "noise" => S::Record(vec![
            req("noise", noise_holder()),
            req("xz_scale", S::Double),
            req("y_scale", S::Double),
            default("shift_x", df(), zero()),
            default("shift_y", df(), zero()),
            default("shift_z", df(), zero()),
        ]),
        "distance_to_point" => S::Record(vec![
            req("point", vec3i_schema(None)),
            req(
                "metric",
                S::Enum(&["euclidean", "euclidean_squared", "manhattan", "chebyshev"]),
            ),
        ]),
        "gradient" => {
            let record = S::Record(vec![
                req("axis", S::Enum(&["x", "y", "z"])),
                default(
                    "tiling",
                    S::Enum(&["clamp_to_edge", "repeat", "mirrored_repeat"]),
                    Value::from("clamp_to_edge"),
                ),
                req("from_coordinate", S::Int),
                req("to_coordinate", S::Int),
                req("from_value", noise_value()),
                req("to_value", noise_value()),
            ]);
            S::custom_partial(move |v, ctx| {
                validate(record.decode(v, ctx), |tag| match tag {
                    Tag::Compound(c)
                        if c.get_int("from_coordinate") == c.get_int("to_coordinate") =>
                    {
                        Err("from_coordinate cannot be equal to to_coordinate".to_owned())
                    }
                    _ => Ok(()),
                })
            })
        }
        "shift_a" | "shift_b" | "shift" => S::Record(vec![req("noise", noise_holder())]),
        "abs" | "square" | "cube" | "sqrt" | "half_negative" | "quarter_negative"
        | "reciprocal" | "negate" | "squeeze" | "log" | "sign" | "cache" | "blend_density" => {
            input()
        }
        "floor" | "round" | "ceil" | "truncate" => S::Record(vec![
            req("input", df()),
            default("multiple", df(), Value::from(1.0)),
        ]),
        "add" | "sub" | "mul" | "div" | "min" | "max" => {
            S::Record(vec![req("left", df()), req("right", df())])
        }
        "pow" => S::Record(vec![req("base", df()), req("exponent", df())]),
        "spline" => S::Record(vec![req("spline", cubic_spline())]),
        "lerp" => S::Record(vec![
            req("alpha", df()),
            req("first", df()),
            req("second", df()),
        ]),
        "clamp" => {
            let record = S::Record(vec![
                req("input", df()),
                req("min", noise_value()),
                req("max", noise_value()),
            ]);
            S::custom_partial(move |v, ctx| {
                validate(record.decode(v, ctx), |tag| match tag {
                    Tag::Compound(c) => match (c.get_float("min"), c.get_float("max")) {
                        (Some(min), Some(max)) if max < min => Err(format!(
                            "min ({}) must be less than or equal to max ({})",
                            java_float_to_string(min),
                            java_float_to_string(max)
                        )),
                        _ => Ok(()),
                    },
                    _ => Ok(()),
                })
            })
        }
        "range_choice" => S::Record(vec![
            req("input", df()),
            req("min_inclusive", noise_value()),
            req("max_exclusive", noise_value()),
            req("when_in_range", df()),
            req("when_out_of_range", df()),
        ]),
        "interval_select" => {
            let record = S::Record(vec![
                req("input", df()),
                req("thresholds", S::list_of(noise_value())),
                req("functions", S::sized_list(df(), 2, i32::MAX as usize)),
            ]);
            S::custom_partial(move |v, ctx| {
                validate(record.decode(v, ctx), |tag| {
                    let Tag::Compound(c) = tag else { return Ok(()) };
                    let thresholds: Vec<f32> = c
                        .get_list("thresholds")
                        .map(|l| {
                            l.iter()
                                .filter_map(|t| {
                                    if let Tag::Float(f) = t {
                                        Some(*f)
                                    } else {
                                        None
                                    }
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let functions = c.get_list("functions").map_or(0, |l| l.len());
                    if thresholds.len() != functions - 1 {
                        Err(format!(
                            "Expected {} thresholds for {functions} functions, but got {}",
                            functions - 1,
                            thresholds.len()
                        ))
                    } else if thresholds.windows(2).any(|w| w[0].total_cmp(&w[1]).is_gt()) {
                        Err("Threshold values must be ordered from smallest to largest".to_owned())
                    } else {
                        Ok(())
                    }
                })
            })
        }
        "interpolated" => S::Record(vec![
            req("input", df()),
            req("cell_size_xz", positive_int()),
            req("cell_size_y", positive_int()),
        ]),
        "slice" => S::Record(vec![
            req("axis", S::Enum(&["x", "y", "z"])),
            req("coordinate", S::Int),
            req("input", df()),
        ]),
        "find_top_surface" => S::Record(vec![
            req("density", df()),
            req("upper_bound", df()),
            req(
                "lower_bound",
                S::IntRange(DIMENSION_MIN_Y * 2, DIMENSION_MAX_Y * 2),
            ),
            req("cell_height", positive_int()),
        ]),
        "old_blended_noise" => {
            let scale = || S::DoubleRange(0.001, 1000.0);
            S::Record(vec![
                req("xz_scale", scale()),
                req("y_scale", scale()),
                req("xz_factor", scale()),
                req("y_factor", scale()),
                req("smear_scale_multiplier", S::DoubleRange(1.0, 8.0)),
            ])
        }
        _ => return None,
    })
}

/// `CubicSpline.codec(DensityFunction.CODEC)`: a constant or a multipoint spline whose points hold sub-splines.
fn cubic_spline() -> Schema {
    let point = Schema::Record(vec![
        req("location", Schema::Float),
        req(
            "value",
            Schema::custom_partial(|v, ctx| cubic_spline().decode(v, ctx)),
        ),
        req("derivative", Schema::Float),
    ]);
    Schema::either(
        Schema::Float,
        Schema::Record(vec![
            req("coordinate", density_function()),
            req("points", Schema::NonEmptyList(Box::new(point))),
        ]),
    )
}

// ---- material rules --------------------------------------------------------------------------------------------

/// `MaterialRule.HOLDER_CODEC`.
fn material_rule() -> Schema {
    Schema::HolderOrDirect {
        registry: keys::MATERIAL_RULE,
        direct: |v, ctx| Schema::dispatch("worldgen/material_rule_type", rule_type).decode(v, ctx),
    }
}

/// `MaterialCondition.CODEC`.
fn material_condition() -> Schema {
    Schema::HolderOrDirect {
        registry: keys::MATERIAL_CONDITION,
        direct: |v, ctx| {
            Schema::dispatch("worldgen/material_condition_type", condition_type).decode(v, ctx)
        },
    }
}

fn rule_type(name: &str) -> Option<Schema> {
    use Schema as S;
    Some(match name.strip_prefix("minecraft:")? {
        "block" => S::Record(vec![req("result_state", block_state_schema())]),
        "bandlands" => S::Record(vec![]),
        "sequence" => S::Record(vec![req("sequence", S::list_of(material_rule()))]),
        "condition" => S::Record(vec![
            req("if_true", material_condition()),
            req("then_run", material_rule()),
        ]),
        "ore_vein" => S::Record(vec![
            req("ore_block", block_state_schema()),
            req("raw_ore_block", block_state_schema()),
            req("filler_block", block_state_schema()),
            req("raw_ore_chance", S::FloatRange(0.0, 1.0)),
            req("density", density_function()),
            req("richness", density_function()),
            req("filler_gap", density_function()),
        ]),
        _ => return None,
    })
}

fn condition_type(name: &str) -> Option<Schema> {
    use Schema as S;
    Some(match name.strip_prefix("minecraft:")? {
        "biome" => S::Record(vec![req(
            "biome_is",
            S::HolderSet {
                registry: keys::BIOME,
                always_list: false,
            },
        )]),
        "noise_threshold" => S::Record(vec![
            req("noise", S::Identifier),
            req("min_threshold", S::Double),
            req("max_threshold", S::Double),
            default("is_3d", S::Bool, Value::Bool(false)),
        ]),
        "vertical_gradient" => S::Record(vec![
            req("random_name", S::Identifier),
            req("true_at_and_below", vertical_anchor_schema()),
            req("false_at_and_above", vertical_anchor_schema()),
        ]),
        "y_above" => S::Record(vec![
            req("anchor", vertical_anchor_schema()),
            req("surface_depth_multiplier", S::IntRange(-20, 20)),
            req("add_stone_depth", S::Bool),
        ]),
        "water" => S::Record(vec![
            req("offset", S::Int),
            req("surface_depth_multiplier", S::IntRange(-20, 20)),
            req("add_stone_depth", S::Bool),
        ]),
        "temperature" | "steep" | "hole" | "above_preliminary_surface" => S::Record(vec![]),
        "not" => S::Record(vec![req("invert", material_condition())]),
        "stone_depth" => S::Record(vec![
            req("offset", S::Int),
            req("add_surface_depth", S::Bool),
            req("secondary_depth_range", S::Int),
            req("surface_type", S::Enum(&["ceiling", "floor"])),
        ]),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::codec::{Failure, Lookup, RegistryLookup};
    use mc_core::ident::Identifier;
    use mc_worldgen::densityfunction::codec::DENSITY_FUNCTION_TYPES;
    use mc_worldgen::material::{MATERIAL_CONDITION_TYPES, MATERIAL_RULE_TYPES};
    use serde_json::json;

    struct Everything;
    impl RegistryLookup for Everything {
        fn contains(&self, _: &str, _: &Identifier) -> Lookup {
            Lookup::Yes
        }
        fn tag_exists(&self, _: &str, _: &Identifier) -> Lookup {
            Lookup::Yes
        }
    }

    fn encode(v: Value) -> Result<Tag, Failure> {
        let lookup = Everything;
        density_function().decode(&v, &RegistryDecodeContext::new(&lookup))
    }

    #[test]
    fn constants_and_defaults_encode_like_the_codec() {
        // a `constant` is its bare float; defaults and equal bounds are dropped
        assert_eq!(
            encode(json!({"type": "minecraft:constant", "value": 1})).unwrap(),
            Tag::Float(1.0)
        );
        assert_eq!(
            encode(json!({"type": "floor", "input": 0.5, "multiple": 1}))
                .unwrap_or_else(|f| panic!("{f:?}")),
            {
                let mut c = mc_nbt::CompoundTag::new();
                c.put("type", Tag::String("minecraft:floor".into()));
                c.put("input", Tag::Float(0.5));
                Tag::Compound(c)
            }
        );
        assert_eq!(
            encode(json!({"type": "minecraft:clamp", "input": 0, "min": 2, "max": 1}))
                .unwrap_err()
                .message,
            "Failed to parse either. First: Not a number: {\"type\":\"minecraft:clamp\",\"input\":0,\"min\":2,\"max\":1}; Second: min (2.0) must be less than or equal to max (1.0)"
        );
    }

    #[test]
    fn unknown_types_name_their_registry() {
        assert_eq!(
            encode(json!({"type": "minecraft:nope"}))
                .unwrap_err()
                .message,
            "Failed to parse either. First: Not a number: {\"type\":\"minecraft:nope\"}; Second: Unknown registry key in ResourceKey[minecraft:root / minecraft:worldgen/density_function_type]: minecraft:nope"
        );
    }

    #[test]
    fn every_decoded_type_has_a_schema() {
        let has = |types: &[&str], schema: fn(&str) -> Option<Schema>| {
            types
                .iter()
                .all(|ty| schema(&format!("minecraft:{ty}")).is_some())
        };
        assert!(has(DENSITY_FUNCTION_TYPES, density_type));
        assert!(has(MATERIAL_CONDITION_TYPES, condition_type));
        assert!(has(MATERIAL_RULE_TYPES, rule_type));
    }
}
