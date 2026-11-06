//! `world.attribute.EnvironmentAttributeMap` (+ `EnvironmentAttributes`, `AttributeTypes`, `AttributeModifier` libraries):
//! the `attributes` object of dimension types and biomes. Values are validated and re-encoded per attribute type; the
//! typed map keeps each attribute's modifier and argument as validated JSON.

use std::sync::Arc;

use indexmap::IndexMap;
use mc_core::ident::Identifier;
use mc_core::math::java::java_float_to_string;
use mc_nbt::{CompoundTag, Tag};
use serde_json::Value;

use crate::registry::codec::lenient;
use crate::registry::codec::{RegistryDecodeContext, Schema, default, default_always, opt, req};

/// `AttributeTypes` (the value type of an attribute).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AttributeKind {
    Boolean,
    TriState,
    Float,
    AngleDegrees,
    RgbColor,
    ArgbColor,
    Integer,
    MoonPhase,
    Activity,
    BedRule,
    Particle,
    AmbientParticles,
    BackgroundMusic,
    AmbientSounds,
    MobSpawnSettings,
}

/// `AttributeRange`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AttributeRange {
    Any,
    Float(f32, f32),
}

/// One `EnvironmentAttributes` constant.
#[derive(Clone, Copy, Debug)]
pub struct AttributeDef {
    pub name: &'static str,
    pub kind: AttributeKind,
    pub range: AttributeRange,
    pub syncable: bool,
    pub positional: bool,
}

use AttributeKind as K;
use AttributeRange as R;

const UNIT: R = R::Float(0.0, 1.0);
const NON_NEGATIVE: R = R::Float(0.0, f32::INFINITY);

const fn def(
    name: &'static str,
    kind: K,
    range: R,
    syncable: bool,
    positional: bool,
) -> AttributeDef {
    AttributeDef {
        name,
        kind,
        range,
        syncable,
        positional,
    }
}

/// Every registered environment attribute (`EnvironmentAttributes`), in registration order.
pub const ATTRIBUTES: &[AttributeDef] = &[
    def("visual/fog_color", K::RgbColor, R::Any, true, true),
    def("visual/fog_start_distance", K::Float, R::Any, true, true),
    def(
        "visual/fog_end_distance",
        K::Float,
        NON_NEGATIVE,
        true,
        true,
    ),
    def(
        "visual/sky_fog_end_distance",
        K::Float,
        NON_NEGATIVE,
        true,
        true,
    ),
    def(
        "visual/cloud_fog_end_distance",
        K::Float,
        NON_NEGATIVE,
        true,
        true,
    ),
    def("visual/water_fog_color", K::RgbColor, R::Any, true, true),
    def(
        "visual/water_fog_start_distance",
        K::Float,
        R::Any,
        true,
        true,
    ),
    def(
        "visual/water_fog_end_distance",
        K::Float,
        NON_NEGATIVE,
        true,
        true,
    ),
    def("visual/sky_color", K::RgbColor, R::Any, true, true),
    def(
        "visual/sunrise_sunset_color",
        K::ArgbColor,
        R::Any,
        true,
        true,
    ),
    def("visual/cloud_color", K::ArgbColor, R::Any, true, true),
    def("visual/cloud_height", K::Float, R::Any, true, true),
    def("visual/sun_angle", K::AngleDegrees, R::Any, true, true),
    def("visual/moon_angle", K::AngleDegrees, R::Any, true, true),
    def("visual/star_angle", K::AngleDegrees, R::Any, true, true),
    def("visual/moon_phase", K::MoonPhase, R::Any, true, true),
    def("visual/star_brightness", K::Float, UNIT, true, true),
    def("visual/block_light_tint", K::RgbColor, R::Any, true, true),
    def("visual/sky_light_color", K::RgbColor, R::Any, true, true),
    def("visual/sky_light_factor", K::Float, UNIT, true, true),
    def("visual/night_vision_color", K::RgbColor, R::Any, true, true),
    def(
        "visual/ambient_light_color",
        K::RgbColor,
        R::Any,
        true,
        true,
    ),
    def(
        "visual/default_dripstone_particle",
        K::Particle,
        R::Any,
        true,
        true,
    ),
    def(
        "visual/ambient_particles",
        K::AmbientParticles,
        R::Any,
        true,
        true,
    ),
    def(
        "audio/background_music",
        K::BackgroundMusic,
        R::Any,
        true,
        true,
    ),
    def("audio/music_volume", K::Float, UNIT, true, true),
    def("audio/ambient_sounds", K::AmbientSounds, R::Any, true, true),
    def("audio/firefly_bush_sounds", K::Boolean, R::Any, true, true),
    def(
        "gameplay/sky_light_level",
        K::Float,
        R::Float(0.0, 15.0),
        true,
        false,
    ),
    def("gameplay/can_start_raid", K::Boolean, R::Any, false, true),
    def("gameplay/water_evaporates", K::Boolean, R::Any, true, true),
    def("gameplay/bed_rule", K::BedRule, R::Any, false, true),
    def("gameplay/straw_bed_rule", K::BedRule, R::Any, false, true),
    def(
        "gameplay/respawn_anchor_works",
        K::Boolean,
        R::Any,
        false,
        true,
    ),
    def(
        "gameplay/nether_portal_spawns_piglin",
        K::Boolean,
        R::Any,
        false,
        true,
    ),
    def("gameplay/fast_lava", K::Boolean, R::Any, true, false),
    def(
        "gameplay/increased_fire_burnout",
        K::Boolean,
        R::Any,
        false,
        true,
    ),
    def("gameplay/eyeblossom_open", K::TriState, R::Any, false, true),
    def(
        "gameplay/turtle_egg_hatch_chance",
        K::Float,
        UNIT,
        false,
        true,
    ),
    def("gameplay/piglins_zombify", K::Boolean, R::Any, true, true),
    def("gameplay/snow_golem_melts", K::Boolean, R::Any, false, true),
    def("gameplay/creaking_active", K::Boolean, R::Any, true, true),
    def(
        "gameplay/surface_slime_spawn_chance",
        K::Float,
        UNIT,
        false,
        true,
    ),
    def(
        "gameplay/cat_waking_up_gift_chance",
        K::Float,
        UNIT,
        false,
        true,
    ),
    def(
        "gameplay/bees_stay_in_hive",
        K::Boolean,
        R::Any,
        false,
        true,
    ),
    def("gameplay/monsters_burn", K::Boolean, R::Any, false, true),
    def(
        "gameplay/can_pillager_patrol_spawn",
        K::Boolean,
        R::Any,
        false,
        true,
    ),
    def(
        "gameplay/natural_mob_spawns",
        K::MobSpawnSettings,
        R::Any,
        false,
        true,
    ),
    def(
        "gameplay/creature_world_gen_spawn_probability",
        K::Float,
        R::Float(0.0, 0.9999999),
        false,
        true,
    ),
    def(
        "gameplay/villager_activity",
        K::Activity,
        R::Any,
        false,
        true,
    ),
    def(
        "gameplay/baby_villager_activity",
        K::Activity,
        R::Any,
        false,
        true,
    ),
];

/// `EnvironmentAttributes` by registry name (`minecraft:gameplay/bed_rule`).
pub fn attribute(id: &Identifier) -> Option<&'static AttributeDef> {
    if id.namespace() != "minecraft" {
        return None;
    }
    ATTRIBUTES.iter().find(|a| a.name == id.path())
}

/// One entry of an `EnvironmentAttributeMap`.
#[derive(Clone, Debug)]
pub struct AttributeEntry {
    /// `AttributeModifier.OperationId` name; `override` is the plain-value form.
    pub modifier: String,
    /// The value (override) or the modifier argument, as validated JSON.
    pub argument: Arc<Value>,
}

/// `EnvironmentAttributeMap`.
#[derive(Clone, Debug, Default)]
pub struct EnvironmentAttributeMap {
    entries: IndexMap<Identifier, AttributeEntry>,
}

impl EnvironmentAttributeMap {
    pub fn get(&self, attribute: &Identifier) -> Option<&AttributeEntry> {
        self.entries.get(attribute)
    }

    pub fn contains(&self, attribute: &Identifier) -> bool {
        self.entries.contains_key(attribute)
    }

    pub fn key_set(&self) -> impl Iterator<Item = &Identifier> {
        self.entries.keys()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The plain value of an attribute set with the `override` modifier.
    pub fn override_value(&self, attribute: &str) -> Option<&Value> {
        let id = Identifier::with_default_namespace(attribute).ok()?;
        self.entries
            .get(&id)
            .filter(|e| e.modifier == "override")
            .map(|e| &*e.argument)
    }

    /// `CODEC` / `NETWORK_CODEC` / `CODEC_ONLY_POSITIONAL` parse of the JSON object, plus the re-encoded NBT (`network` drops
    /// the attributes that are not syncable). Returns the map (all attributes) and the NBT compound.
    pub fn parse(
        json: &Value,
        ctx: &RegistryDecodeContext,
        positional_only: bool,
        network: bool,
    ) -> Result<(EnvironmentAttributeMap, CompoundTag), String> {
        let object = json.as_object().ok_or_else(|| {
            format!(
                "Not a map: {}",
                serde_json::to_string(json).unwrap_or_default()
            )
        })?;
        let mut entries = IndexMap::new();
        let mut nbt = CompoundTag::new();
        let mut not_positional = Vec::new();
        for (key, value) in object {
            let id = Identifier::read(key)?;
            let def = attribute(&id).ok_or_else(|| {
                format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:environment_attribute]: ResourceKey[minecraft:environment_attribute / {id}]")
            })?;
            let (modifier, argument, encoded) = parse_entry(def, value, ctx)?;
            if !def.positional {
                not_positional.push(format!("minecraft:{}", def.name));
            }
            if !network || def.syncable {
                nbt.put(id.to_string(), encoded);
            }
            entries.insert(
                id,
                AttributeEntry {
                    modifier,
                    argument: Arc::new(argument),
                },
            );
        }
        if positional_only && !not_positional.is_empty() {
            return Err(format!(
                "The following attributes cannot be positional: [{}]",
                not_positional.join(", ")
            ));
        }
        Ok((EnvironmentAttributeMap { entries }, nbt))
    }
}

/// `Codec.either(valueCodec, fullCodec)` of `EnvironmentAttributeMap.Entry.createCodec`.
fn parse_entry(
    def: &AttributeDef,
    value: &Value,
    ctx: &RegistryDecodeContext,
) -> Result<(String, Value, Tag), String> {
    let value_schema = value_schema(def.kind);
    let first = value_schema
        .convert(value, ctx)
        .and_then(|tag| validate_range(def, value, tag));
    match first {
        Ok(tag) => Ok(("override".to_owned(), value.clone(), tag)),
        Err(left_error) => {
            let right = (|| {
                let o = value.as_object().ok_or_else(|| {
                    format!(
                        "Not a map: {}",
                        serde_json::to_string(value).unwrap_or_default()
                    )
                })?;
                let modifier = o.get("modifier").and_then(Value::as_str).ok_or_else(|| {
                    format!(
                        "No key modifier in MapLike[{}]",
                        serde_json::to_string(value).unwrap_or_default()
                    )
                })?;
                let allowed = modifiers_of(def.kind);
                if modifier != "override" && !allowed.contains(&modifier) {
                    return Err(format!("Unknown modifier {modifier}"));
                }
                let argument = o.get("argument").ok_or_else(|| {
                    format!(
                        "No key argument in MapLike[{}]",
                        serde_json::to_string(value).unwrap_or_default()
                    )
                })?;
                let arg_tag = if modifier == "override" {
                    value_schema.convert(argument, ctx)?
                } else {
                    argument_schema(def.kind, modifier).convert(argument, ctx)?
                };
                if modifier == "override" {
                    // `entry.modifier == override` encodes as the plain value
                    return Ok(("override".to_owned(), argument.clone(), arg_tag));
                }
                let mut c = CompoundTag::new();
                c.put("modifier", Tag::String(modifier.to_owned()));
                c.put("argument", arg_tag);
                Ok((modifier.to_owned(), argument.clone(), Tag::Compound(c)))
            })();
            right.map_err(|right_error: String| {
                format!("Failed to parse either. First: {left_error}; Second: {right_error}")
            })
        }
    }
}

/// `EnvironmentAttribute.valueCodec()` = type codec `.validate(range)`.
fn validate_range(def: &AttributeDef, _json: &Value, tag: Tag) -> Result<Tag, String> {
    if let (R::Float(min, max), Tag::Float(f)) = (def.range, &tag) {
        if !(*f >= min && *f <= max) {
            return Err(format!(
                "{} is not in range [{}; {}]",
                java_float_to_string(*f),
                java_float_to_string(min),
                java_float_to_string(max)
            ));
        }
    }
    Ok(tag)
}

/// `AttributeModifier` libraries per type.
fn modifiers_of(kind: K) -> &'static [&'static str] {
    match kind {
        K::Boolean => &["and", "nand", "or", "nor", "xor", "xnor"],
        K::Float | K::AngleDegrees => &[
            "alpha_blend",
            "add",
            "subtract",
            "multiply",
            "minimum",
            "maximum",
        ],
        K::RgbColor | K::ArgbColor => &[
            "alpha_blend",
            "add",
            "subtract",
            "multiply",
            "blend_to_gray",
        ],
        K::Integer => &["add", "subtract", "multiply", "minimum", "maximum"],
        K::MobSpawnSettings => &["overlay"],
        K::AmbientParticles => &["append"],
        _ => &[],
    }
}

fn argument_schema(kind: K, modifier: &str) -> Schema {
    match (kind, modifier) {
        (K::Boolean, _) => Schema::Bool,
        (K::Float | K::AngleDegrees, "alpha_blend") => Schema::Record(vec![
            req("value", Schema::Float),
            default("alpha", Schema::FloatRange(0.0, 1.0), Value::from(1.0)),
        ]),
        (K::Float | K::AngleDegrees, _) => Schema::Float,
        (K::RgbColor | K::ArgbColor, "blend_to_gray") => Schema::Record(vec![
            req("brightness", Schema::FloatRange(0.0, 1.0)),
            req("factor", Schema::FloatRange(0.0, 1.0)),
        ]),
        (K::RgbColor, "alpha_blend") | (K::ArgbColor, _) => argb_color(),
        (K::RgbColor, _) => rgb_color(),
        (K::Integer, _) => Schema::Int,
        (K::MobSpawnSettings, _) => mob_spawn_settings(),
        (K::AmbientParticles, _) => value_schema(K::AmbientParticles),
        _ => Schema::Raw,
    }
}

const MOON_PHASES: &[&str] = &[
    "full_moon",
    "waning_gibbous",
    "third_quarter",
    "waning_crescent",
    "new_moon",
    "waxing_crescent",
    "first_quarter",
    "waxing_gibbous",
];
const BED_RULES: &[&str] = &["always", "when_dark", "never"];
const MOB_CATEGORIES: &[&str] = &[
    "monster",
    "creature",
    "ambient",
    "axolotls",
    "underground_water_creature",
    "water_creature",
    "water_ambient",
    "misc",
];

fn sound_event() -> Schema {
    // `SoundEvent.CODEC`: a sound id, or an inline `{sound_id, range?}`
    Schema::custom(|v, ctx| match v {
        Value::String(_) => {
            Schema::StaticRegistry(crate::registry::keys::SOUND_EVENT).convert(v, ctx)
        }
        _ => Schema::Record(vec![
            req("sound_id", Schema::Identifier),
            lenient("range", Schema::Float),
        ])
        .convert(v, ctx),
    })
}

fn music() -> Schema {
    Schema::Record(vec![
        req("sound", sound_event()),
        req("min_delay", Schema::IntRange(0, i32::MAX)),
        req("max_delay", Schema::IntRange(0, i32::MAX)),
        default("replace_current_music", Schema::Bool, Value::Bool(false)),
    ])
}

fn mob_spawn_settings() -> Schema {
    let spawner = Schema::Record(vec![
        req(
            "type",
            Schema::StaticRegistry(crate::registry::keys::ENTITY_TYPE),
        ),
        req("count", super::int_provider::schema(i32::MIN, i32::MAX)),
        default_always("weight", Schema::IntRange(1, i32::MAX), Value::from(1)),
    ]);
    Schema::Record(vec![
        req(
            "spawns_by_category",
            Schema::map_of(Schema::Enum(MOB_CATEGORIES), Schema::list_of(spawner)),
        ),
        req(
            "spawn_costs",
            Schema::map_of(
                Schema::StaticRegistry(crate::registry::keys::ENTITY_TYPE),
                Schema::Record(vec![
                    req("energy_budget", Schema::Double),
                    req("charge", Schema::Double),
                ]),
            ),
        ),
    ])
}

/// `ExtraCodecs.hexColor(digits)` + the alternatives; encodes as `#rrggbb` / `#aarrggbb`.
fn color(digits: usize) -> Schema {
    Schema::custom(move |v, _| {
        let hex = |s: &str| -> Result<u32, String> {
            let rest = s.strip_prefix('#').ok_or("Hex color must begin with #")?;
            if rest.len() != digits {
                return Err(format!(
                    "Hex color is wrong size, expected {digits} digits but got {}",
                    rest.len()
                ));
            }
            u32::from_str_radix(rest, 16).map_err(|_| format!("Invalid color value: {s}"))
        };
        let argb: u32 = match v {
            Value::String(s) => hex(s)?,
            Value::Number(n) => n
                .as_i64()
                .map(|i| i as i32 as u32)
                .or_else(|| n.as_f64().map(|f| f as i32 as u32))
                .ok_or("Not a number")?,
            Value::Array(list)
                if list.len() == digits / 2
                    || (digits == 6 && list.len() == 3)
                    || (digits == 8 && list.len() == 4) =>
            {
                let f: Vec<f32> = list
                    .iter()
                    .map(|x| {
                        x.as_f64()
                            .map(|d| d as f32)
                            .ok_or_else(|| format!("Not a number: {x}"))
                    })
                    .collect::<Result<_, _>>()?;
                let ch = |x: f32| ((x * 255.0).floor() as i32).clamp(0, 255) as u32; // ARGB.as8BitChannel
                if digits == 6 {
                    (ch(f[0]) << 16) | (ch(f[1]) << 8) | ch(f[2])
                } else {
                    (ch(f[3]) << 24) | (ch(f[0]) << 16) | (ch(f[1]) << 8) | ch(f[2])
                }
            }
            other => return Err(format!("Not a string: {other}")),
        };
        Ok(Tag::String(if digits == 6 {
            format!("#{:06x}", argb & 0xFF_FFFF)
        } else {
            format!("#{argb:08x}")
        }))
    })
}

fn rgb_color() -> Schema {
    color(6)
}

fn argb_color() -> Schema {
    color(8)
}

/// `AttributeType.valueCodec()`.
fn value_schema(kind: K) -> Schema {
    match kind {
        K::Boolean => Schema::Bool,
        K::TriState => Schema::custom(|v, ctx| match v {
            Value::Bool(_) => Schema::Bool.convert(v, ctx),
            _ => Schema::Enum(&["true", "false", "default"]).convert(v, ctx),
        }),
        K::Float | K::AngleDegrees => Schema::Float,
        K::RgbColor => rgb_color(),
        K::ArgbColor => argb_color(),
        K::Integer => Schema::Int,
        K::MoonPhase => Schema::Enum(MOON_PHASES),
        K::Activity => Schema::StaticRegistry(crate::registry::keys::ACTIVITY),
        K::BedRule => Schema::Record(vec![
            req("can_sleep", Schema::Enum(BED_RULES)),
            req("can_set_spawn", Schema::Enum(BED_RULES)),
            default("destroy_on_use", Schema::Bool, Value::Bool(false)),
            default("destroy_on_leave", Schema::Bool, Value::Bool(false)),
            opt("error_message", Schema::Text),
        ]),
        K::Particle => particle(),
        K::AmbientParticles => Schema::list_of(Schema::Record(vec![
            req("particle", particle()),
            req("probability", Schema::FloatRange(0.0, 1.0)),
        ])),
        K::BackgroundMusic => Schema::Record(vec![
            opt("default", music()),
            opt("creative", music()),
            opt("underwater", music()),
        ]),
        K::AmbientSounds => Schema::Record(vec![
            opt("loop", sound_event()),
            opt(
                "mood",
                Schema::Record(vec![
                    req("sound", sound_event()),
                    req("tick_delay", Schema::Int),
                    req("block_search_extent", Schema::Int),
                    req("offset", Schema::Double),
                ]),
            ),
            default(
                "additions",
                Schema::custom(|v, ctx| {
                    // `ExtraCodecs.compactListCodec`: a single element encodes bare
                    let one = Schema::Record(vec![
                        req("sound", sound_event()),
                        req("tick_chance", Schema::Double),
                    ]);
                    match v {
                        Value::Array(list) => {
                            let tags = list
                                .iter()
                                .map(|e| one.convert(e, ctx))
                                .collect::<Result<Vec<_>, _>>()?;
                            if tags.len() == 1 {
                                return Ok(tags.into_iter().next().unwrap_or(Tag::End));
                            }
                            let mut out = mc_nbt::ListTag::new();
                            tags.into_iter().for_each(|t| out.push(t));
                            Ok(Tag::List(out))
                        }
                        single => one.convert(single, ctx),
                    }
                }),
                Value::Array(vec![]),
            ),
        ]),
        K::MobSpawnSettings => mob_spawn_settings(),
    }
}

/// `ParticleTypes.CODEC`: `{type, ...options}`; the options of each particle type are not modelled (numbers narrowed).
fn particle() -> Schema {
    Schema::custom(|v, ctx| {
        let o = v.as_object().ok_or_else(|| format!("Not a map: {v}"))?;
        let ty = o
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("No key type in MapLike[{v}]"))?;
        Schema::StaticRegistry("minecraft:particle_type")
            .convert(&Value::String(ty.to_owned()), ctx)?;
        Schema::Raw.convert(v, ctx)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::codec::{Lookup, RegistryLookup};
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

    #[test]
    fn attribute_table_is_complete() {
        let registry = &mc_data::registry::registries::ENVIRONMENT_ATTRIBUTE;
        assert_eq!(ATTRIBUTES.len(), registry.len());
        for (i, a) in ATTRIBUTES.iter().enumerate() {
            assert_eq!(
                registry.name_of(i as u32),
                Some(format!("minecraft:{}", a.name).as_str()),
                "registration order"
            );
        }
        assert!(attribute(&Identifier::parse("minecraft:gameplay/bed_rule").unwrap()).is_some());
    }

    #[test]
    fn map_parse_and_network_filter() {
        let l = L;
        let ctx = RegistryDecodeContext::new(&l);
        let json = json!({
            "minecraft:visual/fog_color": "#c0d8ff",
            "minecraft:gameplay/respawn_anchor_works": false,
            "minecraft:visual/cloud_height": 192.33,
            "minecraft:gameplay/fast_lava": {"modifier": "or", "argument": true},
        });
        let (map, full) = EnvironmentAttributeMap::parse(&json, &ctx, false, false).unwrap();
        assert_eq!(map.len(), 4);
        assert_eq!(full.len(), 4);
        assert_eq!(
            full.get("minecraft:visual/cloud_height"),
            Some(&Tag::Float(192.33))
        );
        assert_eq!(
            full.get("minecraft:visual/fog_color"),
            Some(&Tag::String("#c0d8ff".into()))
        );
        let (_, net) = EnvironmentAttributeMap::parse(&json, &ctx, false, true).unwrap();
        assert_eq!(net.len(), 3); // respawn_anchor_works is not syncable
        let err = EnvironmentAttributeMap::parse(&json, &ctx, true, false).unwrap_err();
        assert_eq!(
            err,
            "The following attributes cannot be positional: [minecraft:gameplay/fast_lava]"
        );
        assert!(
            EnvironmentAttributeMap::parse(&json!({"minecraft:nope": 1}), &ctx, false, false)
                .unwrap_err()
                .starts_with("Unknown registry key")
        );
        assert!(
            EnvironmentAttributeMap::parse(
                &json!({"minecraft:visual/star_brightness": 2.0}),
                &ctx,
                false,
                false
            )
            .is_err()
        );
    }
}
