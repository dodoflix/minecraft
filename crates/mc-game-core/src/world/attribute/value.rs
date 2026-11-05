//! Values of the environment attribute types (`AttributeTypes`): the typed payloads `EnvironmentAttributeSystem` computes
//! with, plus their JSON decoding (the codecs of `AttributeType.valueCodec()` / `AttributeModifier.argumentCodec`).

use indexmap::IndexMap;
use mc_core::ident::Identifier;
use serde_json::Value;
use std::sync::Arc;

/// `net.minecraft.util.TriState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriState {
    True,
    False,
    Default,
}

impl TriState {
    /// `TriState.toBoolean(defaultValue)`.
    pub fn to_boolean(self, default_value: bool) -> bool {
        match self {
            TriState::True => true,
            TriState::False => false,
            TriState::Default => default_value,
        }
    }
}

/// `BedRule.Rule`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BedRuleRule {
    Always,
    WhenDark,
    Never,
}

impl BedRuleRule {
    fn by_name(name: &str) -> Option<BedRuleRule> {
        Some(match name {
            "always" => BedRuleRule::Always,
            "when_dark" => BedRuleRule::WhenDark,
            "never" => BedRuleRule::Never,
            _ => return None,
        })
    }

    /// `BedRule.Rule.test(level)`.
    pub fn test(self, level: &crate::server::level::ServerLevel) -> bool {
        match self {
            BedRuleRule::Always => true,
            BedRuleRule::WhenDark => level.is_dark_outside(),
            BedRuleRule::Never => false,
        }
    }
}

/// `net.minecraft.world.attribute.BedRule`; the error message is kept as its JSON text component.
#[derive(Clone, Debug, PartialEq)]
pub struct BedRule {
    pub can_sleep: BedRuleRule,
    pub can_set_spawn: BedRuleRule,
    pub destroy_on_use: bool,
    pub destroy_on_leave: bool,
    pub error_message: Option<Value>,
}

impl BedRule {
    fn no_sleep_message() -> Option<Value> {
        Some(serde_json::json!({"translate": "block.minecraft.bed.no_sleep"}))
    }

    /// `BedRule.CAN_SLEEP_WHEN_DARK`.
    pub fn can_sleep_when_dark() -> BedRule {
        BedRule { can_sleep: BedRuleRule::WhenDark, can_set_spawn: BedRuleRule::Always, destroy_on_use: false, destroy_on_leave: false, error_message: Self::no_sleep_message() }
    }

    /// `BedRule.DESTROY_ON_USE`.
    pub fn destroy_on_use() -> BedRule {
        BedRule { can_sleep: BedRuleRule::Never, can_set_spawn: BedRuleRule::Never, destroy_on_use: true, destroy_on_leave: false, error_message: None }
    }

    /// `BedRule.DESTROY_ON_LEAVE`.
    pub fn destroy_on_leave() -> BedRule {
        BedRule { can_sleep: BedRuleRule::WhenDark, can_set_spawn: BedRuleRule::Never, destroy_on_use: false, destroy_on_leave: true, error_message: Self::no_sleep_message() }
    }

    /// `canSleep(level)`.
    pub fn can_sleep(&self, level: &crate::server::level::ServerLevel) -> bool {
        self.can_sleep.test(level)
    }

    /// `canSetSpawn(level)`.
    pub fn can_set_spawn(&self, level: &crate::server::level::ServerLevel) -> bool {
        self.can_set_spawn.test(level)
    }

    /// `errorMessage()` as a component.
    pub fn error_message(&self) -> Option<mc_text::Component> {
        self.error_message.as_ref().and_then(|json| mc_text::Component::from_json(json).ok())
    }

    fn from_json(json: &Value) -> Option<BedRule> {
        let o = json.as_object()?;
        Some(BedRule {
            can_sleep: BedRuleRule::by_name(o.get("can_sleep")?.as_str()?)?,
            can_set_spawn: BedRuleRule::by_name(o.get("can_set_spawn")?.as_str()?)?,
            destroy_on_use: o.get("destroy_on_use").map_or(Some(false), Value::as_bool)?,
            destroy_on_leave: o.get("destroy_on_leave").map_or(Some(false), Value::as_bool)?,
            error_message: o.get("error_message").cloned(),
        })
    }
}

/// `MobCategory` names in ordinal order (the iteration order of `MobSpawnSettingsModifier.Overlay`).
pub const MOB_CATEGORIES: [&str; 8] = ["monster", "creature", "ambient", "axolotls", "underground_water_creature", "water_creature", "water_ambient", "misc"];

/// `MobSpawnSettings` with the spawner entries (`{type, count, weight}`) and spawn costs kept as validated JSON.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MobSpawnSettings {
    /// `spawnsByCategory` (a category present with an empty list means "defined, no spawns").
    pub spawns_by_category: IndexMap<String, Vec<Value>>,
    /// `mobSpawnCosts`: entity type id -> `{energy_budget, charge}`.
    pub spawn_costs: IndexMap<String, Value>,
}

impl MobSpawnSettings {
    /// `getMobsToSpawn(category)` (the entries of the category, empty when undefined).
    pub fn get_mobs_to_spawn(&self, category: &str) -> &[Value] {
        self.spawns_by_category.get(category).map_or(&[], Vec::as_slice)
    }

    fn from_json(json: &Value) -> Option<MobSpawnSettings> {
        let o = json.as_object()?;
        let mut spawns_by_category = IndexMap::new();
        for (category, list) in o.get("spawns_by_category")?.as_object()? {
            spawns_by_category.insert(category.clone(), list.as_array()?.clone());
        }
        let mut spawn_costs = IndexMap::new();
        for (ty, cost) in o.get("spawn_costs")?.as_object()? {
            spawn_costs.insert(ty.clone(), cost.clone());
        }
        Some(MobSpawnSettings { spawns_by_category, spawn_costs })
    }

    /// `MobSpawnSettingsModifier.Overlay.apply(first, second)`.
    pub fn overlay(first: &Arc<MobSpawnSettings>, second: &Arc<MobSpawnSettings>) -> Arc<MobSpawnSettings> {
        if first.spawns_by_category.is_empty() && first.spawn_costs.is_empty() {
            return second.clone();
        }
        if second.spawns_by_category.is_empty() && second.spawn_costs.is_empty() {
            return first.clone();
        }
        if first.spawns_by_category.keys().all(|c| second.spawns_by_category.contains_key(c)) && first.spawn_costs.keys().all(|t| second.spawn_costs.contains_key(t)) {
            return second.clone();
        }
        let mut out = MobSpawnSettings::default();
        for category in MOB_CATEGORIES {
            if let Some(list) = second.spawns_by_category.get(category).or_else(|| first.spawns_by_category.get(category)) {
                out.spawns_by_category.insert(category.to_owned(), list.clone());
            }
        }
        for (ty, cost) in first.spawn_costs.iter().chain(second.spawn_costs.iter()) {
            out.spawn_costs.insert(ty.clone(), cost.clone());
        }
        Arc::new(out)
    }
}

/// A value of an attribute type (`Value` of `EnvironmentAttribute<Value>`). `Float` also carries `angle_degrees`.
#[derive(Clone, Debug, PartialEq)]
pub enum AttrValue {
    Bool(bool),
    TriState(TriState),
    Float(f32),
    Int(i32),
    /// `MoonPhase` ordinal (0 = full moon).
    MoonPhase(u8),
    Activity(Identifier),
    BedRule(Arc<BedRule>),
    /// `Vector3fc` (rgb_color).
    Rgb([f32; 3]),
    /// `Vector4fc` (argb_color, x/y/z = r/g/b, w = alpha).
    Argb([f32; 4]),
    Particle(Arc<Value>),
    AmbientParticles(Arc<Vec<Value>>),
    BackgroundMusic(Arc<Value>),
    AmbientSounds(Arc<Value>),
    MobSpawnSettings(Arc<MobSpawnSettings>),
}

pub const MOON_PHASES: [&str; 8] =
    ["full_moon", "waning_gibbous", "third_quarter", "waning_crescent", "new_moon", "waxing_crescent", "first_quarter", "waxing_gibbous"];

impl AttrValue {
    pub fn as_bool(&self) -> bool {
        matches!(self, AttrValue::Bool(true))
    }

    pub fn as_float(&self) -> f32 {
        match self {
            AttrValue::Float(f) => *f,
            _ => 0.0,
        }
    }

    /// `AttributeType.toFloat` (float / angle / integer types).
    pub fn to_float(&self) -> Option<f32> {
        match self {
            AttrValue::Float(f) => Some(*f),
            AttrValue::Int(i) => Some(*i as f32),
            _ => None,
        }
    }

    /// `AttributeType.toInt` (integer type only).
    pub fn to_int(&self) -> Option<i32> {
        match self {
            AttrValue::Int(i) => Some(*i),
            _ => None,
        }
    }
}

/// `ARGB.from8BitChannel(channel)`.
fn channel(c: i32) -> f32 {
    c as f32 / 255.0
}

/// `ARGB.vector3fFromRGB24`.
pub fn vector3_from_rgb24(color: i32) -> [f32; 3] {
    [channel(color >> 16 & 0xFF), channel(color >> 8 & 0xFF), channel(color & 0xFF)]
}

/// `ARGB.vector4fFromARGB32`.
pub fn vector4_from_argb32(color: i32) -> [f32; 4] {
    [channel(color >> 16 & 0xFF), channel(color >> 8 & 0xFF), channel(color & 0xFF), channel(((color as u32) >> 24) as i32)]
}

/// `ARGB.as8BitChannel(value)` = `Mth.floor(value * 255.0F)`.
pub fn as_8bit_channel(value: f32) -> i32 {
    mc_core::math::mth::floor_f32(value * 255.0)
}

/// `ARGB.colorFromFloat(a, r, g, b)`.
pub fn color_from_float(a: f32, r: f32, g: f32, b: f32) -> i32 {
    as_8bit_channel(a) << 24 | as_8bit_channel(r) << 16 | as_8bit_channel(g) << 8 | as_8bit_channel(b)
}

fn hex_color(text: &str, digits: usize) -> Option<i32> {
    let rest = text.strip_prefix('#')?;
    if rest.len() != digits {
        return None;
    }
    let parsed = u32::from_str_radix(rest, 16).ok()?;
    // `ExtraCodecs.hexColor` + `ARGB::opaque` for 6 digits
    Some(if digits == 6 { (parsed | 0xFF00_0000) as i32 } else { parsed as i32 })
}

fn float_list(json: &Value, n: usize) -> Option<Vec<f32>> {
    let list = json.as_array()?;
    if list.len() != n {
        return None;
    }
    list.iter().map(|x| x.as_f64().map(|d| d as f32)).collect()
}

/// `ExtraCodecs.STRING_RGB_VEC3_COLOR`.
pub fn parse_rgb(json: &Value) -> Option<[f32; 3]> {
    match json {
        Value::String(s) => hex_color(s, 6).map(vector3_from_rgb24),
        Value::Array(_) => float_list(json, 3).map(|f| [f[0], f[1], f[2]]),
        Value::Number(n) => n.as_i64().map(|i| vector3_from_rgb24(i as i32)),
        _ => None,
    }
}

/// `ExtraCodecs.STRING_ARGB_VEC4_COLOR`.
pub fn parse_argb(json: &Value) -> Option<[f32; 4]> {
    match json {
        Value::String(s) => hex_color(s, 8).map(vector4_from_argb32),
        Value::Array(_) => float_list(json, 4).map(|f| [f[0], f[1], f[2], f[3]]),
        Value::Number(n) => n.as_i64().map(|i| vector4_from_argb32(i as i32)),
        _ => None,
    }
}

impl AttrValue {
    /// `AttributeType.valueCodec()` of `kind` over validated JSON (`None` for a value the codec would reject).
    pub fn from_json(kind: crate::registry::typed::attributes::AttributeKind, json: &Value) -> Option<AttrValue> {
        use crate::registry::typed::attributes::AttributeKind as K;
        Some(match kind {
            K::Boolean => AttrValue::Bool(json.as_bool()?),
            K::TriState => AttrValue::TriState(match json {
                Value::Bool(true) => TriState::True,
                Value::Bool(false) => TriState::False,
                Value::String(s) => match s.as_str() {
                    "true" => TriState::True,
                    "false" => TriState::False,
                    "default" => TriState::Default,
                    _ => return None,
                },
                _ => return None,
            }),
            K::Float | K::AngleDegrees => AttrValue::Float(json.as_f64()? as f32),
            K::Integer => AttrValue::Int(json.as_i64()? as i32),
            K::RgbColor => AttrValue::Rgb(parse_rgb(json)?),
            K::ArgbColor => AttrValue::Argb(parse_argb(json)?),
            K::MoonPhase => AttrValue::MoonPhase(MOON_PHASES.iter().position(|p| Some(*p) == json.as_str())? as u8),
            K::Activity => AttrValue::Activity(Identifier::try_parse(json.as_str()?)?),
            K::BedRule => AttrValue::BedRule(Arc::new(BedRule::from_json(json)?)),
            K::Particle => AttrValue::Particle(Arc::new(json.clone())),
            K::AmbientParticles => AttrValue::AmbientParticles(Arc::new(json.as_array()?.clone())),
            K::BackgroundMusic => AttrValue::BackgroundMusic(Arc::new(json.clone())),
            K::AmbientSounds => AttrValue::AmbientSounds(Arc::new(json.clone())),
            K::MobSpawnSettings => AttrValue::MobSpawnSettings(Arc::new(MobSpawnSettings::from_json(json)?)),
        })
    }
}
