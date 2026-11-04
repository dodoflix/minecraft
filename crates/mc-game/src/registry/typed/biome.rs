//! `world.level.biome.Biome`: the `worldgen/biome` registry (`DIRECT_CODEC` for loading, `NETWORK_CODEC` for the wire).

use mc_nbt::Tag;
use serde_json::Value;

use super::attributes::EnvironmentAttributeMap;
use super::{HolderSetRef, get_bool, get_f32, get_str};
use crate::registry::codec::{RegistryDecodeContext, Schema, default, req};
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::keys;

/// `Biome.TemperatureModifier`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TemperatureModifier {
    None,
    Frozen,
}

/// `BiomeSpecialEffects.GrassColorModifier`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GrassColorModifier {
    None,
    DarkForest,
    Swamp,
}

/// `BiomeSpecialEffects`: colors as ARGB ints (`ExtraCodecs.STRING_RGB_COLOR` yields an opaque color).
#[derive(Clone, Debug, PartialEq)]
pub struct BiomeSpecialEffects {
    pub water_color: i32,
    pub foliage_color_override: Option<i32>,
    pub dry_foliage_color_override: Option<i32>,
    pub grass_color_override: Option<i32>,
    pub grass_color_modifier: GrassColorModifier,
}

/// `BiomeGenerationSettings`: carvers and the per-decoration-step placed feature lists.
#[derive(Clone, Debug, PartialEq)]
pub struct BiomeGenerationSettings {
    pub carvers: HolderSetRef,
    pub features: Vec<HolderSetRef>,
}

/// `Biome` (`climate_settings`, `attributes`, `effects`, `generation_settings`).
#[derive(Clone, Debug)]
pub struct Biome {
    pub has_precipitation: bool,
    pub temperature: f32,
    pub temperature_modifier: TemperatureModifier,
    pub downfall: f32,
    pub attributes: EnvironmentAttributeMap,
    pub special_effects: BiomeSpecialEffects,
    pub generation_settings: BiomeGenerationSettings,
}

fn color_schema() -> Schema {
    // `ExtraCodecs.STRING_RGB_COLOR`
    Schema::custom(|v, ctx| {
        let tag = match v {
            Value::String(_) | Value::Number(_) | Value::Array(_) => parse_rgb(v).map(|c| Tag::String(format!("#{:06x}", c as u32 & 0xFF_FFFF)))?,
            other => return Err(format!("Not a string: {other}")),
        };
        let _ = ctx;
        Ok(tag)
    })
}

/// `STRING_RGB_COLOR` decode: `"#rrggbb"` (opaque), an int, or `[r, g, b]` floats.
fn parse_rgb(v: &Value) -> Result<i32, String> {
    match v {
        Value::String(s) => {
            let rest = s.strip_prefix('#').ok_or("Hex color must begin with #")?;
            if rest.len() != 6 {
                return Err(format!("Hex color is wrong size, expected 6 digits but got {}", rest.len()));
            }
            let value = u32::from_str_radix(rest, 16).map_err(|_| format!("Invalid color value: {s}"))?;
            Ok((0xFF00_0000u32 | value) as i32)
        }
        Value::Number(n) => n.as_i64().map(|i| i as i32).or_else(|| n.as_f64().map(|f| f as i32)).ok_or_else(|| "Not a number".to_owned()),
        Value::Array(l) if l.len() == 3 => {
            let ch = |x: &Value| x.as_f64().map(|d| ((d as f32 * 255.0).floor() as i32).clamp(0, 255)).ok_or_else(|| format!("Not a number: {x}"));
            Ok((0xFF << 24) | (ch(&l[0])? << 16) | (ch(&l[1])? << 8) | ch(&l[2])?)
        }
        other => Err(format!("Not a string: {other}")),
    }
}

fn climate_fields() -> Vec<crate::registry::codec::Field> {
    vec![
        req("has_precipitation", Schema::Bool),
        req("temperature", Schema::Float),
        default("temperature_modifier", Schema::Enum(&["none", "frozen"]), Value::from("none")),
        req("downfall", Schema::Float),
    ]
}

fn effects_schema() -> Schema {
    Schema::Record(vec![
        req("water_color", color_schema()),
        crate::registry::codec::opt("foliage_color", color_schema()),
        crate::registry::codec::opt("dry_foliage_color", color_schema()),
        crate::registry::codec::opt("grass_color", color_schema()),
        default("grass_color_modifier", Schema::Enum(&["none", "dark_forest", "swamp"]), Value::from("none")),
    ])
}

fn generation_fields() -> Vec<crate::registry::codec::Field> {
    vec![
        req("carvers", Schema::HolderSet { registry: keys::CARVER, always_list: false }),
        req("features", Schema::list_of(Schema::HolderSet { registry: keys::PLACED_FEATURE, always_list: false })),
    ]
}

/// Parse `json` with the full (`direct`) or the network biome codec and return the NBT compound (record fields flattened).
fn convert(json: &Value, ctx: &RegistryDecodeContext, direct: bool) -> Result<Tag, String> {
    let mut fields = climate_fields();
    fields.push(req("effects", effects_schema()));
    if direct {
        fields.extend(generation_fields());
    }
    let Tag::Compound(mut nbt) = Schema::Record(fields).convert(json, ctx)? else { return Err("Not a map".to_owned()) };
    if let Some(attributes) = json.get("attributes") {
        // direct: positional attributes only; network: syncable only
        let (_, attr_nbt) = EnvironmentAttributeMap::parse(attributes, ctx, direct, !direct)?;
        if attr_nbt.len() > 0 {
            nbt.put("attributes", Tag::Compound(attr_nbt));
        }
    }
    Ok(Tag::Compound(nbt))
}

impl Biome {
    /// `DIRECT_CODEC` decode.
    pub fn from_json(json: &Value, ctx: &RegistryDecodeContext) -> Result<Biome, String> {
        convert(json, ctx, true)?;
        let attributes = match json.get("attributes") {
            Some(a) => EnvironmentAttributeMap::parse(a, ctx, true, false)?.0,
            None => EnvironmentAttributeMap::default(),
        };
        let effects = json.get("effects").unwrap_or(&Value::Null);
        let color = |key: &str| effects.get(key).and_then(|v| parse_rgb(v).ok());
        Ok(Biome {
            has_precipitation: get_bool(json, "has_precipitation").unwrap_or(false),
            temperature: get_f32(json, "temperature").unwrap_or(0.0),
            temperature_modifier: if get_str(json, "temperature_modifier") == Some("frozen") { TemperatureModifier::Frozen } else { TemperatureModifier::None },
            downfall: get_f32(json, "downfall").unwrap_or(0.0),
            attributes,
            special_effects: BiomeSpecialEffects {
                water_color: color("water_color").unwrap_or(0),
                foliage_color_override: color("foliage_color"),
                dry_foliage_color_override: color("dry_foliage_color"),
                grass_color_override: color("grass_color"),
                grass_color_modifier: match get_str(effects, "grass_color_modifier") {
                    Some("dark_forest") => GrassColorModifier::DarkForest,
                    Some("swamp") => GrassColorModifier::Swamp,
                    _ => GrassColorModifier::None,
                },
            },
            generation_settings: BiomeGenerationSettings {
                carvers: json.get("carvers").and_then(HolderSetRef::from_json).unwrap_or(HolderSetRef::Elements(Vec::new())),
                features: json
                    .get("features")
                    .and_then(Value::as_array)
                    .map(|l| l.iter().filter_map(HolderSetRef::from_json).collect())
                    .unwrap_or_default(),
            },
        })
    }

    pub fn has_precipitation(&self) -> bool {
        self.has_precipitation
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    let biome = Biome::from_json(json, ctx)?;
    // `SYNCHRONIZED_REGISTRIES` sends `Biome.NETWORK_CODEC` (no carvers/features, only syncable attributes)
    let network = convert(json, ctx, false)?;
    Ok(DecodedEntry::with_network(biome, network))
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::BIOME, decode };
