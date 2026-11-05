use super::easing::EasingType;
use crate::registry::typed::attributes::{AttributeKind, AttributeRange, EnvironmentAttributeMap, attribute};
use mc_core::ident::Identifier;
use mc_core::math::mth;
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::HashMap;

/// A value of a supported attribute type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AttrValue {
    Float(f32),
    Bool(bool),
    /// `MoonPhase` ordinal (0 = full moon).
    MoonPhase(u8),
}

/// A modifier argument (`AttributeModifier.argumentCodec`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Arg {
    Float(f32),
    FloatAlpha { value: f32, alpha: f32 },
    Bool(bool),
    MoonPhase(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Modifier {
    Override,
    Add,
    Subtract,
    Multiply,
    Minimum,
    Maximum,
    AlphaBlend,
    And,
    Nand,
    Or,
    Nor,
    Xor,
    Xnor,
}

impl Modifier {
    fn by_name(name: &str) -> Option<Modifier> {
        Some(match name {
            "override" => Modifier::Override,
            "add" => Modifier::Add,
            "subtract" => Modifier::Subtract,
            "multiply" => Modifier::Multiply,
            "minimum" => Modifier::Minimum,
            "maximum" => Modifier::Maximum,
            "alpha_blend" => Modifier::AlphaBlend,
            "and" => Modifier::And,
            "nand" => Modifier::Nand,
            "or" => Modifier::Or,
            "nor" => Modifier::Nor,
            "xor" => Modifier::Xor,
            "xnor" => Modifier::Xnor,
            _ => return None,
        })
    }

    /// `AttributeModifier.apply(subject, argument)`.
    fn apply(self, subject: AttrValue, argument: Arg) -> AttrValue {
        match (self, subject, argument) {
            (Modifier::Override, _, Arg::Float(v)) => AttrValue::Float(v),
            (Modifier::Override, _, Arg::Bool(v)) => AttrValue::Bool(v),
            (Modifier::Override, _, Arg::MoonPhase(v)) => AttrValue::MoonPhase(v),
            (Modifier::AlphaBlend, AttrValue::Float(s), Arg::FloatAlpha { value, alpha }) => AttrValue::Float(mth::lerp_f32(alpha, s, value)),
            (Modifier::Add, AttrValue::Float(s), Arg::Float(a)) => AttrValue::Float(s + a),
            (Modifier::Subtract, AttrValue::Float(s), Arg::Float(a)) => AttrValue::Float(s - a),
            (Modifier::Multiply, AttrValue::Float(s), Arg::Float(a)) => AttrValue::Float(s * a),
            (Modifier::Minimum, AttrValue::Float(s), Arg::Float(a)) => AttrValue::Float(java_min(s, a)),
            (Modifier::Maximum, AttrValue::Float(s), Arg::Float(a)) => AttrValue::Float(java_max(s, a)),
            (m, AttrValue::Bool(s), Arg::Bool(a)) => AttrValue::Bool(match m {
                Modifier::And => a && s,
                Modifier::Nand => !a || !s,
                Modifier::Or => a || s,
                Modifier::Nor => !a && !s,
                Modifier::Xor => a ^ s,
                Modifier::Xnor => a == s,
                _ => s,
            }),
            _ => subject,
        }
    }
}

fn java_min(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() { f32::NAN } else if a == 0.0 && b == 0.0 { if a.is_sign_negative() { a } else { b } } else { a.min(b) }
}
fn java_max(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() { f32::NAN } else if a == 0.0 && b == 0.0 { if a.is_sign_negative() { b } else { a } } else { a.max(b) }
}

const MOON_PHASES: [&str; 8] =
    ["full_moon", "waning_gibbous", "third_quarter", "waning_crescent", "new_moon", "waxing_crescent", "first_quarter", "waxing_gibbous"];

/// Parse a modifier argument (or an override value) of `kind`.
fn parse_arg(kind: AttributeKind, modifier: Modifier, value: &Value) -> Option<Arg> {
    match kind {
        AttributeKind::Float if modifier == Modifier::AlphaBlend => {
            let o = value.as_object()?;
            Some(Arg::FloatAlpha { value: o.get("value")?.as_f64()? as f32, alpha: o.get("alpha").map_or(Some(1.0), |a| a.as_f64().map(|f| f as f32))? })
        }
        AttributeKind::Float => Some(Arg::Float(value.as_f64()? as f32)),
        AttributeKind::Boolean => Some(Arg::Bool(value.as_bool()?)),
        AttributeKind::MoonPhase => {
            let name = value.as_str()?;
            MOON_PHASES.iter().position(|p| *p == name).map(|i| Arg::MoonPhase(i as u8))
        }
        _ => None,
    }
}

fn default_value(kind: AttributeKind, name: &str) -> Option<AttrValue> {
    Some(match (kind, name) {
        (AttributeKind::MoonPhase, _) => AttrValue::MoonPhase(0),
        (AttributeKind::Float, "gameplay/sky_light_level") => AttrValue::Float(15.0),
        (AttributeKind::Float, "visual/cloud_height") => AttrValue::Float(192.33),
        (AttributeKind::Float, "visual/fog_end_distance") => AttrValue::Float(1024.0),
        (AttributeKind::Float, "visual/sky_fog_end_distance") => AttrValue::Float(512.0),
        (AttributeKind::Float, "visual/cloud_fog_end_distance") => AttrValue::Float(2048.0),
        (AttributeKind::Float, "visual/water_fog_start_distance") => AttrValue::Float(-8.0),
        (AttributeKind::Float, "visual/water_fog_end_distance") => AttrValue::Float(96.0),
        (AttributeKind::Float, "visual/sky_light_factor" | "audio/music_volume") => AttrValue::Float(1.0),
        (AttributeKind::Float, "gameplay/turtle_egg_hatch_chance") => AttrValue::Float(0.002),
        (AttributeKind::Float, _) => AttrValue::Float(0.0),
        (AttributeKind::Boolean, "gameplay/can_start_raid" | "gameplay/piglins_zombify") => AttrValue::Bool(true),
        (AttributeKind::Boolean, _) => AttrValue::Bool(false),
        _ => return None,
    })
}

/// `EnvironmentAttributeLayer`.
#[derive(Clone, Debug)]
enum Layer {
    Constant(Modifier, Arg),
    Track(Track),
    /// `WeatherAttributes` layer: the rain and thunder entries of the attribute.
    Weather { rain: Option<(Modifier, Arg)>, thunder: Option<(Modifier, Arg)> },
}

/// `AttributeTrackSampler` + `KeyframeTrackSampler`.
#[derive(Clone, Debug)]
struct Track {
    clock: Identifier,
    modifier: Modifier,
    period_ticks: Option<i32>,
    easing: EasingType,
    /// Keyframe interpolation: `true` = `ofFloat` (and the alpha-blend pair), `false` = constant step.
    interpolated: bool,
    segments: Vec<Segment>,
}

#[derive(Clone, Debug)]
struct Segment {
    easing: EasingType,
    from_value: Arg,
    from_ticks: i32,
    to_value: Arg,
    to_ticks: i32,
}

const CONSTANT_EASING: EasingType = EasingType::Named(|_| 0.0, "constant");

impl Track {
    fn new(clock: Identifier, modifier: Modifier, period: Option<i32>, easing: EasingType, interpolated: bool, keyframes: Vec<(i32, Arg)>) -> Track {
        let mut segments = Vec::new();
        if keyframes.len() == 1 {
            let v = keyframes[0].1;
            segments.push(Segment { easing: CONSTANT_EASING, from_value: v, from_ticks: 0, to_value: v, to_ticks: 0 });
        } else {
            let seg = |from: (i32, Arg), from_ticks: i32, to: (i32, Arg), to_ticks: i32| Segment { easing: easing.clone(), from_value: from.1, from_ticks, to_value: to.1, to_ticks };
            let (first, last) = (keyframes[0], keyframes[keyframes.len() - 1]);
            if let Some(p) = period {
                segments.push(seg(last, last.0 - p, first, first.0));
            }
            for w in keyframes.windows(2) {
                segments.push(seg(w[0], w[0].0, w[1], w[1].0));
            }
            if let Some(p) = period {
                segments.push(seg(last, last.0, first, first.0 + p));
            }
        }
        Track { clock, modifier, period_ticks: period, easing, interpolated, segments }
    }

    fn lerp(&self, alpha: f32, from: Arg, to: Arg) -> Arg {
        if !self.interpolated {
            return if alpha >= 1.0 { to } else { from };
        }
        match (from, to) {
            (Arg::Float(a), Arg::Float(b)) => Arg::Float(mth::lerp_f32(alpha, a, b)),
            (Arg::FloatAlpha { value: v0, alpha: a0 }, Arg::FloatAlpha { value: v1, alpha: a1 }) => {
                Arg::FloatAlpha { value: mth::lerp_f32(alpha, v0, v1), alpha: mth::lerp_f32(alpha, a0, a1) }
            }
            _ => from,
        }
    }

    /// `KeyframeTrackSampler.sample(ticks)`.
    fn sample(&self, ticks: i64) -> Arg {
        let sample_ticks = match self.period_ticks {
            Some(p) => ticks.rem_euclid(i64::from(p)),
            None => ticks,
        };
        let segment = self.segments.iter().find(|s| sample_ticks < i64::from(s.to_ticks)).unwrap_or_else(|| self.segments.last().expect("a track has keyframes"));
        if sample_ticks <= i64::from(segment.from_ticks) {
            segment.from_value
        } else if sample_ticks >= i64::from(segment.to_ticks) {
            segment.to_value
        } else {
            let alpha = (sample_ticks - i64::from(segment.from_ticks)) as f32 / (segment.to_ticks - segment.from_ticks) as f32;
            let eased = segment.easing.apply(alpha);
            self.lerp(eased, segment.from_value, segment.to_value)
        }
    }
}

/// A decoded `Timeline` (`clock`, `period_ticks`, the tracks of supported attribute types).
#[derive(Clone, Debug)]
pub struct Timeline {
    tracks: Vec<(&'static str, AttributeKind, Track)>,
}

impl Timeline {
    /// `Timeline.DIRECT_CODEC` for the tracks of supported types (others are client presentation / not evaluated here).
    pub fn from_json(json: &Value) -> Option<Timeline> {
        let clock = Identifier::try_parse(json.get("clock")?.as_str()?)?;
        let period = json.get("period_ticks").and_then(Value::as_i64).map(|p| p as i32);
        let mut tracks = Vec::new();
        for (key, track) in json.get("tracks")?.as_object()? {
            let id = Identifier::try_parse(key)?;
            let Some(def) = attribute(&id) else { continue };
            if !matches!(def.kind, AttributeKind::Float | AttributeKind::Boolean | AttributeKind::MoonPhase) {
                continue;
            }
            let modifier = match track.get("modifier").and_then(Value::as_str) {
                Some(name) => Modifier::by_name(name)?,
                None => Modifier::Override,
            };
            let easing = match track.get("ease") {
                Some(e) => EasingType::from_json(e)?,
                None => EasingType::LINEAR,
            };
            let keyframes: Vec<(i32, Arg)> = track
                .get("keyframes")?
                .as_array()?
                .iter()
                .map(|k| Some((k.get("ticks")?.as_i64()? as i32, parse_arg(def.kind, modifier, k.get("value")?)?)))
                .collect::<Option<_>>()?;
            if keyframes.is_empty() {
                return None;
            }
            let interpolated = def.kind == AttributeKind::Float;
            tracks.push((def.name, def.kind, Track::new(clock.clone(), modifier, period, easing, interpolated, keyframes)));
        }
        Some(Timeline { tracks })
    }
}

/// What the system reads at evaluation time.
pub struct AttributeEnv<'a> {
    /// `clockManager.getInstance(clock).totalTicks()`.
    pub clock_total_ticks: &'a dyn Fn(&Identifier) -> i64,
    /// `level.getRainLevel(1.0F)` / `getThunderLevel(1.0F)`.
    pub rain_level: f32,
    pub thunder_level: f32,
}

struct Sampler {
    kind: AttributeKind,
    range: AttributeRange,
    base: AttrValue,
    layers: Vec<Layer>,
}

/// `EnvironmentAttributeSystem` for one level (dimension and timeline layers, builtin weather layers).
pub struct EnvironmentAttributeSystem {
    samplers: HashMap<&'static str, Sampler>,
    /// `ValueSampler.cachedTickValue`: valid until [`EnvironmentAttributeSystem::invalidate_tick_cache`].
    cache: Mutex<HashMap<&'static str, AttrValue>>,
}

impl EnvironmentAttributeSystem {
    /// `builder().addDefaultLayers(level)`: the dimension constant layer, the dimension's timelines (in order), then the
    /// weather layers if the level can have weather. (Positional biome layers only affect presentation attributes.)
    pub fn build(dimension_attributes: &EnvironmentAttributeMap, timelines: &[Timeline], can_have_weather: bool) -> EnvironmentAttributeSystem {
        let mut layers: HashMap<&'static str, (AttributeKind, AttributeRange, Vec<Layer>)> = HashMap::new();
        let mut add = |name: &'static str, kind: AttributeKind, range: AttributeRange, layer: Layer| {
            layers.entry(name).or_insert_with(|| (kind, range, Vec::new())).2.push(layer);
        };
        for id in dimension_attributes.key_set() {
            let Some(def) = attribute(id) else { continue };
            let Some(entry) = dimension_attributes.get(id) else { continue };
            let Some(modifier) = Modifier::by_name(&entry.modifier) else { continue };
            if let Some(arg) = parse_arg(def.kind, modifier, &entry.argument) {
                add(def.name, def.kind, def.range, Layer::Constant(modifier, arg));
            }
        }
        for timeline in timelines {
            for (name, kind, track) in &timeline.tracks {
                let def = crate::registry::typed::attributes::ATTRIBUTES.iter().find(|d| d.name == *name).expect("known attribute");
                add(name, *kind, def.range, Layer::Track(track.clone()));
            }
        }
        if can_have_weather {
            let rain_thunder = |name: &'static str, kind: AttributeKind, range: AttributeRange, rain: (Modifier, Arg), thunder: (Modifier, Arg)| {
                (name, kind, range, Layer::Weather { rain: Some(rain), thunder: Some(thunder) })
            };
            let blend = |value: f32, alpha: f32| (Modifier::AlphaBlend, Arg::FloatAlpha { value, alpha });
            let set_f = |v: f32| (Modifier::Override, Arg::Float(v));
            let set_b = |v: bool| (Modifier::Override, Arg::Bool(v));
            let builtin = [
                rain_thunder("gameplay/sky_light_level", AttributeKind::Float, AttributeRange::Float(0.0, 15.0), blend(4.0, 0.3125), blend(4.0, 0.52734375)),
                rain_thunder("visual/sky_light_factor", AttributeKind::Float, AttributeRange::Float(0.0, 1.0), blend(0.24, 0.3125), blend(0.24, 0.52734375)),
                rain_thunder("visual/star_brightness", AttributeKind::Float, AttributeRange::Float(0.0, 1.0), set_f(0.0), set_f(0.0)),
                rain_thunder("gameplay/bees_stay_in_hive", AttributeKind::Boolean, AttributeRange::Any, set_b(true), set_b(true)),
            ];
            for (name, kind, range, layer) in builtin {
                add(name, kind, range, layer);
            }
        }
        let samplers = layers
            .into_iter()
            .filter_map(|(name, (kind, range, mut list))| {
                let mut base = default_value(kind, name)?;
                // `bakeLayerSampler`: leading constant layers fold into the base value
                while let Some(Layer::Constant(modifier, arg)) = list.first().cloned() {
                    base = modifier.apply(base, arg);
                    list.remove(0);
                }
                Some((name, Sampler { kind, range, base, layers: list }))
            })
            .collect();
        EnvironmentAttributeSystem { samplers, cache: Mutex::new(HashMap::new()) }
    }

    /// Evaluates every sampler into the per-tick cache so later reads without a clock source (block code) see the
    /// values of this tick (vanilla caches each attribute's value for the whole tick on first read).
    pub fn prime(&self, env: &AttributeEnv<'_>) {
        let mut cache = self.cache.lock();
        for (name, sampler) in &self.samplers {
            cache.insert(name, self.compute(name, sampler, env));
        }
    }

    /// `invalidateTickCache()` (start of `ServerLevel.tick`).
    pub fn invalidate_tick_cache(&self) {
        self.cache.lock().clear();
    }

    fn compute(&self, name: &'static str, sampler: &Sampler, env: &AttributeEnv<'_>) -> AttrValue {
        let mut result = sampler.base;
        for layer in &sampler.layers {
            match layer {
                Layer::Constant(m, a) => result = m.apply(result, *a),
                Layer::Track(t) => result = t.modifier.apply(result, t.sample((env.clock_total_ticks)(&t.clock))),
                Layer::Weather { rain, thunder } => {
                    let thunder_level = env.thunder_level;
                    let rain_level = env.rain_level - thunder_level;
                    for (level, entry) in [(rain_level, rain), (thunder_level, thunder)] {
                        if let Some((m, a)) = entry
                            && level > 0.0
                        {
                            let to = m.apply(result, *a);
                            result = state_change_lerp(level, result, to);
                        }
                    }
                }
            }
        }
        let _ = name;
        sanitize(sampler.range, result)
    }

    /// `getDimensionValue(attribute)` (cached per tick); `None` for attributes of unsupported types.
    pub fn get_dimension_value(&self, name: &'static str, env: &AttributeEnv<'_>) -> Option<AttrValue> {
        if let Some(v) = self.cache.lock().get(name) {
            return Some(*v);
        }
        let value = match self.samplers.get(name) {
            Some(sampler) => self.compute(name, sampler, env),
            None => {
                let def = crate::registry::typed::attributes::ATTRIBUTES.iter().find(|d| d.name == name)?;
                default_value(def.kind, name)?
            }
        };
        self.cache.lock().insert(name, value);
        Some(value)
    }

    pub fn get_float(&self, name: &'static str, env: &AttributeEnv<'_>) -> f32 {
        match self.get_dimension_value(name, env) {
            Some(AttrValue::Float(f)) => f,
            _ => 0.0,
        }
    }

    pub fn get_bool(&self, name: &'static str, env: &AttributeEnv<'_>) -> bool {
        matches!(self.get_dimension_value(name, env), Some(AttrValue::Bool(true)))
    }

    /// `MOON_PHASE` ordinal.
    pub fn get_moon_phase(&self, env: &AttributeEnv<'_>) -> u8 {
        match self.get_dimension_value("visual/moon_phase", env) {
            Some(AttrValue::MoonPhase(p)) => p,
            _ => 0,
        }
    }

    pub fn kind_of(&self, name: &str) -> Option<AttributeKind> {
        self.samplers.get(name).map(|s| s.kind)
    }
}

/// `AttributeType.stateChangeLerp`: floats lerp, the others step (`ofStep(0.0)`).
fn state_change_lerp(alpha: f32, from: AttrValue, to: AttrValue) -> AttrValue {
    match (from, to) {
        (AttrValue::Float(a), AttrValue::Float(b)) => AttrValue::Float(mth::lerp_f32(alpha, a, b)),
        _ => to,
    }
}

fn sanitize(range: AttributeRange, value: AttrValue) -> AttrValue {
    match (range, value) {
        (AttributeRange::Float(min, max), AttrValue::Float(v)) => AttrValue::Float(if v >= min && v <= max { v } else { mth::clamp_f32(v, min, max) }),
        _ => value,
    }
}

/// `Float.compare`-free helper for tests.
#[cfg(test)]
mod tests {
    use super::*;

    fn day_timeline() -> Timeline {
        Timeline::from_json(&serde_json::json!({
            "clock": "minecraft:overworld", "period_ticks": 24000,
            "tracks": {"minecraft:gameplay/sky_light_level": {"modifier": "multiply", "keyframes": [
                {"ticks": 133, "value": 1.0}, {"ticks": 11867, "value": 1.0}, {"ticks": 13670, "value": 0.26666668}, {"ticks": 22330, "value": 0.26666668}]}}
        }))
        .unwrap()
    }

    fn level_at(system: &EnvironmentAttributeSystem, ticks: i64, rain: f32, thunder: f32) -> f32 {
        system.invalidate_tick_cache();
        let clock = move |_: &Identifier| ticks;
        system.get_float("gameplay/sky_light_level", &AttributeEnv { clock_total_ticks: &clock, rain_level: rain, thunder_level: thunder })
    }

    #[test]
    fn sky_light_follows_the_day_track() {
        let system = EnvironmentAttributeSystem::build(&EnvironmentAttributeMap::default(), &[day_timeline()], true);
        assert_eq!(level_at(&system, 6000, 0.0, 0.0), 15.0);
        assert_eq!(level_at(&system, 18000, 0.0, 0.0), 15.0 * 0.26666668f32);
        // halfway through the dusk ramp (11867..13670)
        let mid = level_at(&system, 12768, 0.0, 0.0);
        assert!(mid > 15.0 * 0.26666668 && mid < 15.0, "{mid}");
        // wraps around the period: 24000 + 6000 is noon again
        assert_eq!(level_at(&system, 30000, 0.0, 0.0), 15.0);
    }

    #[test]
    fn rain_and_thunder_blend_toward_four() {
        let system = EnvironmentAttributeSystem::build(&EnvironmentAttributeMap::default(), &[day_timeline()], true);
        let rain = level_at(&system, 6000, 1.0, 0.0);
        assert_eq!(rain, mth::lerp_f32(0.3125, 15.0, 4.0));
        let thunder = level_at(&system, 6000, 1.0, 1.0);
        assert_eq!(thunder, mth::lerp_f32(0.52734375, 15.0, 4.0));
        // no weather layers in a dimension without weather
        let no_weather = EnvironmentAttributeSystem::build(&EnvironmentAttributeMap::default(), &[day_timeline()], false);
        assert_eq!(level_at(&no_weather, 6000, 1.0, 1.0), 15.0);
    }

    #[test]
    fn moon_phase_steps_per_day() {
        let moon = Timeline::from_json(&serde_json::json!({
            "clock": "minecraft:overworld", "period_ticks": 192000,
            "tracks": {"minecraft:visual/moon_phase": {"keyframes": [
                {"ticks": 0, "value": "full_moon"}, {"ticks": 24000, "value": "waning_gibbous"}, {"ticks": 48000, "value": "third_quarter"}]}}
        }))
        .unwrap();
        let system = EnvironmentAttributeSystem::build(&EnvironmentAttributeMap::default(), &[moon], false);
        let phase = |t: i64| {
            system.invalidate_tick_cache();
            let clock = move |_: &Identifier| t;
            system.get_moon_phase(&AttributeEnv { clock_total_ticks: &clock, rain_level: 0.0, thunder_level: 0.0 })
        };
        assert_eq!(phase(0), 0);
        assert_eq!(phase(23999), 0);
        assert_eq!(phase(24000), 1);
        assert_eq!(phase(48001), 2);
    }
}
