//! `EnvironmentAttributeSystem` (+ `AttributeModifier`s, `LerpFunction`s, `Timeline` track samplers, `WeatherAttributes`)
//! for one level: per attribute a constant base value folded from the leading constant layers, then the remaining layers
//! in vanilla's order (biome positional layers, timeline tracks in dimension order, weather layers).

use super::easing::EasingType;
use super::value::{
    AttrValue, BedRule, MobSpawnSettings, TriState, as_8bit_channel, color_from_float,
    vector3_from_rgb24, vector4_from_argb32,
};
use crate::registry::typed::attributes::{
    ATTRIBUTES, AttributeKind, AttributeRange, EnvironmentAttributeMap, attribute,
};
use crate::world::clock::ClockTotals;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_world::palette::BiomeId;
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// A modifier argument (`AttributeModifier.argumentCodec`).
#[derive(Clone, Debug, PartialEq)]
enum Arg {
    /// The attribute's value type (override), or a simple argument of the subject's type / the color / list argument types.
    Value(AttrValue),
    /// `FloatWithAlpha`.
    FloatAlpha { value: f32, alpha: f32 },
    /// `ColorModifier.BlendToGray`.
    BlendToGray { brightness: f32, factor: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Modifier {
    Override,
    AlphaBlend,
    Add,
    Subtract,
    Multiply,
    BlendToGray,
    Minimum,
    Maximum,
    And,
    Nand,
    Or,
    Nor,
    Xor,
    Xnor,
    Append,
    Overlay,
}

impl Modifier {
    fn by_name(name: &str) -> Option<Modifier> {
        Some(match name {
            "override" => Modifier::Override,
            "alpha_blend" => Modifier::AlphaBlend,
            "add" => Modifier::Add,
            "subtract" => Modifier::Subtract,
            "multiply" => Modifier::Multiply,
            "blend_to_gray" => Modifier::BlendToGray,
            "minimum" => Modifier::Minimum,
            "maximum" => Modifier::Maximum,
            "and" => Modifier::And,
            "nand" => Modifier::Nand,
            "or" => Modifier::Or,
            "nor" => Modifier::Nor,
            "xor" => Modifier::Xor,
            "xnor" => Modifier::Xnor,
            "append" => Modifier::Append,
            "overlay" => Modifier::Overlay,
            _ => return None,
        })
    }

    /// `AttributeModifier.apply(subject, argument)`.
    fn apply(self, subject: &AttrValue, argument: &Arg) -> AttrValue {
        use AttrValue as V;
        match (self, subject, argument) {
            (Modifier::Override, _, Arg::Value(v)) => v.clone(),
            (Modifier::AlphaBlend, V::Float(s), Arg::FloatAlpha { value, alpha }) => {
                V::Float(mth::lerp_f32(*alpha, *s, *value))
            }
            (Modifier::Add, V::Float(s), Arg::Value(V::Float(a))) => V::Float(s + a),
            (Modifier::Subtract, V::Float(s), Arg::Value(V::Float(a))) => V::Float(s - a),
            (Modifier::Multiply, V::Float(s), Arg::Value(V::Float(a))) => V::Float(s * a),
            (Modifier::Minimum, V::Float(s), Arg::Value(V::Float(a))) => V::Float(java_min(*s, *a)),
            (Modifier::Maximum, V::Float(s), Arg::Value(V::Float(a))) => V::Float(java_max(*s, *a)),
            (Modifier::Add, V::Int(s), Arg::Value(V::Int(a))) => V::Int(s.wrapping_add(*a)),
            (Modifier::Subtract, V::Int(s), Arg::Value(V::Int(a))) => V::Int(s.wrapping_sub(*a)),
            (Modifier::Multiply, V::Int(s), Arg::Value(V::Int(a))) => V::Int(s.wrapping_mul(*a)),
            (Modifier::Minimum, V::Int(s), Arg::Value(V::Int(a))) => V::Int(*s.min(a)),
            (Modifier::Maximum, V::Int(s), Arg::Value(V::Int(a))) => V::Int(*s.max(a)),
            (m, V::Bool(s), Arg::Value(V::Bool(a))) => V::Bool(match m {
                Modifier::And => *a && *s,
                Modifier::Nand => !*a || !*s,
                Modifier::Or => *a || *s,
                Modifier::Nor => !*a && !*s,
                Modifier::Xor => *a ^ *s,
                Modifier::Xnor => *a == *s,
                _ => *s,
            }),
            (Modifier::AlphaBlend, V::Rgb(d), Arg::Value(V::Argb(src))) => {
                V::Rgb(alpha_blend_rgb(*d, *src))
            }
            (Modifier::AlphaBlend, V::Argb(d), Arg::Value(V::Argb(src))) => {
                V::Argb(alpha_blend_argb(*d, *src))
            }
            (Modifier::Add, V::Rgb(l), Arg::Value(V::Rgb(r))) => V::Rgb([
                (l[0] + r[0]).min(1.0),
                (l[1] + r[1]).min(1.0),
                (l[2] + r[2]).min(1.0),
            ]),
            (Modifier::Add, V::Argb(l), Arg::Value(V::Rgb(r))) => V::Argb([
                (l[0] + r[0]).min(1.0),
                (l[1] + r[1]).min(1.0),
                (l[2] + r[2]).min(1.0),
                l[3],
            ]),
            (Modifier::Subtract, V::Rgb(l), Arg::Value(V::Rgb(r))) => V::Rgb([
                (l[0] - r[0]).max(0.0),
                (l[1] - r[1]).max(0.0),
                (l[2] - r[2]).max(0.0),
            ]),
            (Modifier::Subtract, V::Argb(l), Arg::Value(V::Rgb(r))) => V::Argb([
                (l[0] - r[0]).max(0.0),
                (l[1] - r[1]).max(0.0),
                (l[2] - r[2]).max(0.0),
                l[3],
            ]),
            (Modifier::Multiply, V::Rgb(l), Arg::Value(V::Rgb(r))) => V::Rgb(if *l == [1.0; 3] {
                *r
            } else if *r == [1.0; 3] {
                *l
            } else {
                [l[0] * r[0], l[1] * r[1], l[2] * r[2]]
            }),
            (Modifier::Multiply, V::Argb(l), Arg::Value(V::Argb(r))) => {
                V::Argb(if *l == [1.0; 4] {
                    *r
                } else if *r == [1.0; 4] {
                    *l
                } else {
                    [l[0] * r[0], l[1] * r[1], l[2] * r[2], l[3] * r[3]]
                })
            }
            (Modifier::BlendToGray, V::Rgb(s), Arg::BlendToGray { brightness, factor }) => {
                let g = s[0] * 0.3 + s[1] * 0.59 + s[2] * 0.11;
                let scaled = [(g * brightness).clamp(0.0, 1.0); 3];
                V::Rgb(lerp3(*factor, *s, scaled))
            }
            (Modifier::BlendToGray, V::Argb(s), Arg::BlendToGray { brightness, factor }) => {
                let g = s[0] * 0.3 + s[1] * 0.59 + s[2] * 0.11;
                let c = (g * brightness).clamp(0.0, 1.0);
                V::Argb(lerp4(*factor, *s, [c, c, c, s[3]]))
            }
            (Modifier::Append, V::AmbientParticles(s), Arg::Value(V::AmbientParticles(a))) => {
                if a.is_empty() {
                    subject.clone()
                } else if s.is_empty() {
                    V::AmbientParticles(a.clone())
                } else {
                    V::AmbientParticles(Arc::new(s.iter().chain(a.iter()).cloned().collect()))
                }
            }
            (Modifier::Overlay, V::MobSpawnSettings(a), Arg::Value(V::MobSpawnSettings(b))) => {
                V::MobSpawnSettings(MobSpawnSettings::overlay(a, b))
            }
            _ => subject.clone(),
        }
    }
}

impl Arg {
    fn into_value(self) -> AttrValue {
        match self {
            Arg::Value(v) => v,
            _ => unreachable!("only value arguments are converted"),
        }
    }
}

fn java_min(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        f32::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() { a } else { b }
    } else {
        a.min(b)
    }
}
fn java_max(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        f32::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() { b } else { a }
    } else {
        a.max(b)
    }
}

/// JOML `Vector3f.lerp(other, t)`: `x + (other.x - x) * t`.
fn lerp3(t: f32, a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}
fn lerp4(t: f32, a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

/// `ARGB.alphaBlend(Vector3fc, Vector4fc)`.
fn alpha_blend_rgb(dest: [f32; 3], src: [f32; 4]) -> [f32; 3] {
    if src[3] == 0.0 {
        dest
    } else {
        lerp3(src[3], dest, [src[0], src[1], src[2]])
    }
}

/// `ARGB.alphaBlend(Vector4fc, Vector4fc)`.
fn alpha_blend_argb(dest: [f32; 4], src: [f32; 4]) -> [f32; 4] {
    let (dest_alpha, source_alpha) = (dest[3], src[3]);
    if source_alpha == 1.0 {
        return src;
    }
    if source_alpha == 0.0 {
        return dest;
    }
    let alpha = source_alpha + dest_alpha * (1.0 - source_alpha);
    let ch = |d: f32, s: f32| (s * source_alpha + d * (alpha - source_alpha)) / alpha;
    [
        ch(dest[0], src[0]),
        ch(dest[1], src[1]),
        ch(dest[2], src[2]),
        alpha,
    ]
}

/// `AttributeType.keyframeLerp` / the modifier's `argumentKeyframeLerp`: interpolates floats, integers, colors and the
/// modifier argument records; every other type steps at alpha 1.
fn lerp_arg(alpha: f32, from: &Arg, to: &Arg) -> Arg {
    use AttrValue as V;
    match (from, to) {
        (Arg::Value(V::Float(a)), Arg::Value(V::Float(b))) => {
            Arg::Value(V::Float(mth::lerp_f32(alpha, *a, *b)))
        }
        (Arg::Value(V::Int(a)), Arg::Value(V::Int(b))) => Arg::Value(V::Int(
            a.wrapping_add(mth::floor_f32(alpha * b.wrapping_sub(*a) as f32)),
        )),
        (Arg::Value(V::Rgb(a)), Arg::Value(V::Rgb(b))) => Arg::Value(V::Rgb(lerp3(alpha, *a, *b))),
        (Arg::Value(V::Argb(a)), Arg::Value(V::Argb(b))) => {
            Arg::Value(V::Argb(lerp4(alpha, *a, *b)))
        }
        (
            Arg::FloatAlpha {
                value: v0,
                alpha: a0,
            },
            Arg::FloatAlpha {
                value: v1,
                alpha: a1,
            },
        ) => Arg::FloatAlpha {
            value: mth::lerp_f32(alpha, *v0, *v1),
            alpha: mth::lerp_f32(alpha, *a0, *a1),
        },
        (
            Arg::BlendToGray {
                brightness: b0,
                factor: f0,
            },
            Arg::BlendToGray {
                brightness: b1,
                factor: f1,
            },
        ) => Arg::BlendToGray {
            brightness: mth::lerp_f32(alpha, *b0, *b1),
            factor: mth::lerp_f32(alpha, *f0, *f1),
        },
        (Arg::Value(V::AmbientParticles(a)), Arg::Value(V::AmbientParticles(b))) => {
            Arg::Value(V::AmbientParticles(cross_fade(alpha, a, b)))
        }
        _ => {
            if alpha >= 1.0 {
                to.clone()
            } else {
                from.clone()
            }
        }
    }
}

/// `LerpFunction.ofListCrossFade` with the `AmbientParticle` probability scaler.
fn cross_fade(alpha: f32, from: &Arc<Vec<Value>>, to: &Arc<Vec<Value>>) -> Arc<Vec<Value>> {
    if alpha == 0.0 {
        return from.clone();
    }
    if alpha == 1.0 {
        return to.clone();
    }
    let scale = |p: &Value, a: f32| {
        let mut p = p.clone();
        if let Some(prob) = p.get("probability").and_then(Value::as_f64) {
            p["probability"] = Value::from(f64::from(prob as f32 * a));
        }
        p
    };
    Arc::new(
        from.iter()
            .map(|p| scale(p, 1.0 - alpha))
            .chain(to.iter().map(|p| scale(p, alpha)))
            .collect(),
    )
}

/// `AttributeType.stateChangeLerp`: interpolated types blend, the others step (`ofStep(0.0)`: the new value).
fn state_change_lerp(alpha: f32, from: &AttrValue, to: &AttrValue) -> AttrValue {
    use AttrValue as V;
    match (from, to) {
        (V::Float(_), V::Float(_))
        | (V::Int(_), V::Int(_))
        | (V::Rgb(_), V::Rgb(_))
        | (V::Argb(_), V::Argb(_))
        | (V::AmbientParticles(_), V::AmbientParticles(_)) => {
            lerp_arg(alpha, &Arg::Value(from.clone()), &Arg::Value(to.clone())).into_value()
        }
        _ => to.clone(),
    }
}

/// Parse a modifier argument (or an override value) of `kind`.
fn parse_arg(kind: AttributeKind, modifier: Modifier, json: &Value) -> Option<Arg> {
    use AttributeKind as K;
    if modifier == Modifier::Override {
        return AttrValue::from_json(kind, json).map(Arg::Value);
    }
    Some(match (kind, modifier) {
        (K::Float | K::AngleDegrees, Modifier::AlphaBlend) => match json {
            Value::Object(o) => Arg::FloatAlpha {
                value: o.get("value")?.as_f64()? as f32,
                alpha: o
                    .get("alpha")
                    .map_or(Some(1.0), |a| a.as_f64().map(|f| f as f32))?,
            },
            other => Arg::FloatAlpha {
                value: other.as_f64()? as f32,
                alpha: 1.0,
            },
        },
        (K::Float | K::AngleDegrees, _) => Arg::Value(AttrValue::Float(json.as_f64()? as f32)),
        (K::Integer, _) => Arg::Value(AttrValue::Int(json.as_i64()? as i32)),
        (K::Boolean, _) => Arg::Value(AttrValue::Bool(json.as_bool()?)),
        (K::RgbColor | K::ArgbColor, Modifier::BlendToGray) => {
            let o = json.as_object()?;
            Arg::BlendToGray {
                brightness: o.get("brightness")?.as_f64()? as f32,
                factor: o.get("factor")?.as_f64()? as f32,
            }
        }
        (K::RgbColor | K::ArgbColor, Modifier::AlphaBlend) | (K::ArgbColor, Modifier::Multiply) => {
            Arg::Value(AttrValue::Argb(super::value::parse_argb(json)?))
        }
        (K::RgbColor | K::ArgbColor, _) => {
            Arg::Value(AttrValue::Rgb(super::value::parse_rgb(json)?))
        }
        (K::AmbientParticles, _) => Arg::Value(AttrValue::from_json(K::AmbientParticles, json)?),
        (K::MobSpawnSettings, _) => Arg::Value(AttrValue::from_json(K::MobSpawnSettings, json)?),
        _ => return None,
    })
}

/// `EnvironmentAttributeMap.Entry`: the plain value (override) or `{modifier, argument}`.
fn parse_entry(kind: AttributeKind, json: &Value) -> Option<(Modifier, Arg)> {
    if let Some(v) = AttrValue::from_json(kind, json) {
        return Some((Modifier::Override, Arg::Value(v)));
    }
    let o = json.as_object()?;
    let modifier = Modifier::by_name(o.get("modifier")?.as_str()?)?;
    Some((modifier, parse_arg(kind, modifier, o.get("argument")?)?))
}

/// `AttributeType` default values (`EnvironmentAttributes` constants).
fn default_value(name: &str, kind: AttributeKind) -> AttrValue {
    use AttrValue as V;
    match name {
        "visual/water_fog_color" => V::Rgb(vector3_from_rgb24(-16448205)),
        "visual/block_light_tint" => V::Rgb(vector3_from_rgb24(-10100)),
        "visual/sky_light_color" => V::Rgb(vector3_from_rgb24(-1)),
        "visual/night_vision_color" => V::Rgb(vector3_from_rgb24(-6710887)),
        "visual/fog_end_distance" => V::Float(1024.0),
        "visual/sky_fog_end_distance" => V::Float(512.0),
        "visual/cloud_fog_end_distance" => V::Float(2048.0),
        "visual/water_fog_start_distance" => V::Float(-8.0),
        "visual/water_fog_end_distance" => V::Float(96.0),
        "visual/cloud_height" => V::Float(192.33),
        "visual/sky_light_factor" | "audio/music_volume" => V::Float(1.0),
        "visual/default_dripstone_particle" => {
            V::Particle(Arc::new(Value::from("minecraft:dripping_dripstone_water")))
        }
        "gameplay/sky_light_level" => V::Float(15.0),
        "gameplay/can_start_raid"
        | "gameplay/piglins_zombify"
        | "gameplay/can_pillager_patrol_spawn" => V::Bool(true),
        "gameplay/bed_rule" => V::BedRule(Arc::new(BedRule::can_sleep_when_dark())),
        "gameplay/straw_bed_rule" => V::BedRule(Arc::new(BedRule::destroy_on_leave())),
        "gameplay/turtle_egg_hatch_chance" => V::Float(0.002),
        "gameplay/creature_world_gen_spawn_probability" => V::Float(0.1),
        "gameplay/villager_activity" | "gameplay/baby_villager_activity" => {
            V::Activity(Identifier::parse("minecraft:idle").expect("static id"))
        }
        _ => match kind {
            AttributeKind::Boolean => V::Bool(false),
            AttributeKind::TriState => V::TriState(TriState::Default),
            AttributeKind::Float | AttributeKind::AngleDegrees => V::Float(0.0),
            AttributeKind::Integer => V::Int(0),
            AttributeKind::MoonPhase => V::MoonPhase(0),
            AttributeKind::RgbColor => V::Rgb([0.0; 3]),
            AttributeKind::ArgbColor => V::Argb([0.0; 4]),
            AttributeKind::AmbientParticles => V::AmbientParticles(Arc::new(Vec::new())),
            AttributeKind::BackgroundMusic => {
                V::BackgroundMusic(Arc::new(Value::Object(Default::default())))
            }
            AttributeKind::AmbientSounds => {
                V::AmbientSounds(Arc::new(Value::Object(Default::default())))
            }
            AttributeKind::MobSpawnSettings => {
                V::MobSpawnSettings(Arc::new(MobSpawnSettings::default()))
            }
            AttributeKind::Particle => V::Particle(Arc::new(Value::Null)),
            AttributeKind::Activity => {
                V::Activity(Identifier::parse("minecraft:idle").expect("static id"))
            }
            AttributeKind::BedRule => V::BedRule(Arc::new(BedRule::can_sleep_when_dark())),
        },
    }
}

fn sanitize(range: AttributeRange, value: AttrValue) -> AttrValue {
    match (range, value) {
        (AttributeRange::Float(min, max), AttrValue::Float(v)) => {
            AttrValue::Float(if v >= min && v <= max {
                v
            } else {
                mth::clamp_f32(v, min, max)
            })
        }
        (_, value) => value,
    }
}

const CONSTANT_EASING: EasingType = EasingType::Named(|_| 0.0, "constant");

#[derive(Clone, Debug)]
struct Segment {
    easing: EasingType,
    from_value: Arg,
    from_ticks: i32,
    to_value: Arg,
    to_ticks: i32,
}

/// `AttributeTrackSampler` + `KeyframeTrackSampler`: the sampled argument is cached per `invalidateTickCache` generation.
#[derive(Debug)]
struct TrackSampler {
    clock: Identifier,
    modifier: Modifier,
    period_ticks: Option<i32>,
    segments: Vec<Segment>,
    cache: Mutex<Option<(u64, Arg)>>,
}

impl TrackSampler {
    fn new(
        clock: Identifier,
        modifier: Modifier,
        period: Option<i32>,
        easing: EasingType,
        keyframes: Vec<(i32, Arg)>,
    ) -> TrackSampler {
        let mut segments = Vec::new();
        if keyframes.len() == 1 {
            let v = keyframes[0].1.clone();
            segments.push(Segment {
                easing: CONSTANT_EASING,
                from_value: v.clone(),
                from_ticks: 0,
                to_value: v,
                to_ticks: 0,
            });
        } else {
            let seg =
                |from: &(i32, Arg), from_ticks: i32, to: &(i32, Arg), to_ticks: i32| Segment {
                    easing: easing.clone(),
                    from_value: from.1.clone(),
                    from_ticks,
                    to_value: to.1.clone(),
                    to_ticks,
                };
            let (first, last) = (&keyframes[0], &keyframes[keyframes.len() - 1]);
            if let Some(p) = period {
                segments.push(seg(last, last.0 - p, first, first.0));
            }
            for w in keyframes.windows(2) {
                segments.push(seg(&w[0], w[0].0, &w[1], w[1].0));
            }
            if let Some(p) = period {
                segments.push(seg(last, last.0, first, first.0 + p));
            }
        }
        TrackSampler {
            clock,
            modifier,
            period_ticks: period,
            segments,
            cache: Mutex::new(None),
        }
    }

    /// `KeyframeTrackSampler.sample(ticks)`.
    fn sample(&self, ticks: i64) -> Arg {
        let sample_ticks = match self.period_ticks {
            Some(p) => ticks.rem_euclid(i64::from(p)),
            None => ticks,
        };
        let segment = self
            .segments
            .iter()
            .find(|s| sample_ticks < i64::from(s.to_ticks))
            .unwrap_or_else(|| self.segments.last().expect("a track has keyframes"));
        if sample_ticks <= i64::from(segment.from_ticks) {
            segment.from_value.clone()
        } else if sample_ticks >= i64::from(segment.to_ticks) {
            segment.to_value.clone()
        } else {
            let alpha = (sample_ticks - i64::from(segment.from_ticks)) as f32
                / (segment.to_ticks - segment.from_ticks) as f32;
            lerp_arg(
                segment.easing.apply(alpha),
                &segment.from_value,
                &segment.to_value,
            )
        }
    }

    /// `AttributeTrackSampler.applyTimeBased(baseValue, cacheTickId)`.
    fn apply(&self, base: &AttrValue, cache_tick_id: u64, clocks: &ClockTotals) -> AttrValue {
        let mut cache = self.cache.lock();
        let argument = match &*cache {
            Some((id, argument)) if *id == cache_tick_id => argument.clone(),
            _ => {
                let argument = self.sample(clocks.total_ticks(&self.clock));
                *cache = Some((cache_tick_id, argument.clone()));
                argument
            }
        };
        self.modifier.apply(base, &argument)
    }
}

/// A decoded `Timeline` (`clock`, `period_ticks`, the tracks).
#[derive(Clone, Debug)]
pub struct Timeline {
    tracks: Vec<(&'static str, AttributeKind, Arc<TrackSampler>)>,
}

impl Timeline {
    /// `Timeline.DIRECT_CODEC` (tracks of unknown attributes are skipped).
    pub fn from_json(json: &Value) -> Option<Timeline> {
        let clock = Identifier::try_parse(json.get("clock")?.as_str()?)?;
        let period = json
            .get("period_ticks")
            .and_then(Value::as_i64)
            .map(|p| p as i32);
        let mut tracks = Vec::new();
        for (key, track) in json
            .get("tracks")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            let id = Identifier::try_parse(key)?;
            let Some(def) = attribute(&id) else { continue };
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
                .map(|k| {
                    Some((
                        k.get("ticks")?.as_i64()? as i32,
                        parse_arg(def.kind, modifier, k.get("value")?)?,
                    ))
                })
                .collect::<Option<_>>()?;
            if keyframes.is_empty() {
                return None;
            }
            tracks.push((
                def.name,
                def.kind,
                Arc::new(TrackSampler::new(
                    clock.clone(),
                    modifier,
                    period,
                    easing,
                    keyframes,
                )),
            ));
        }
        Some(Timeline { tracks })
    }
}

/// `EnvironmentAttributeLayer`.
#[derive(Clone, Debug)]
enum Layer {
    Constant(Modifier, Arg),
    /// A biome layer: the entry of each biome (by id) for this attribute.
    Positional(Arc<Vec<Option<(Modifier, Arg)>>>),
    Track(Arc<TrackSampler>),
    /// `WeatherAttributes` layer: the rain and thunder entries of the attribute.
    Weather {
        rain: Option<(Modifier, Arg)>,
        thunder: Option<(Modifier, Arg)>,
    },
}

/// What the system reads at evaluation time.
pub struct AttributeEnv<'a> {
    /// `level.getRainLevel(1.0F)` / `getThunderLevel(1.0F)`.
    pub rain_level: f32,
    pub thunder_level: f32,
    /// `biomeManager.getBiome(floor(pos))` (`true`: the attribute's `fullResolutionBiomes`) or
    /// `getNoiseBiomeAtPosition(pos)`; `None` when the chunk is not loaded.
    pub biome_at: &'a dyn Fn([f64; 3], bool) -> Option<BiomeId>,
}

struct Sampler {
    range: AttributeRange,
    base: AttrValue,
    layers: Vec<Layer>,
    is_affected_by_position: bool,
    full_resolution_biomes: bool,
    /// `ValueSampler.cachedTickValue`.
    cached: Mutex<Option<AttrValue>>,
}

/// `EnvironmentAttributeSystem` for one level.
pub struct EnvironmentAttributeSystem {
    samplers: HashMap<&'static str, Sampler>,
    defaults: HashMap<&'static str, AttrValue>,
    clocks: Arc<ClockTotals>,
    /// `ValueSampler.cacheTickId` (bumped by `invalidateTickCache`).
    cache_tick_id: AtomicU64,
}

/// The per-biome attribute entries (`Biome.getAttributes()`), in `BiomeId` order.
pub type BiomeAttributes<'a> = Vec<Option<&'a Value>>;

impl EnvironmentAttributeSystem {
    /// `clockManager.getTotalTicks(clock)` of this level's clocks.
    pub fn clock_total_ticks(&self, clock: &Identifier) -> i64 {
        self.clocks.total_ticks(clock)
    }

    /// `builder().addDefaultLayers(level)`: the dimension constant layer, one positional layer per attribute any biome
    /// provides, the dimension's timelines (in order), then the weather layers if the level can have weather.
    pub fn build(
        dimension_attributes: &EnvironmentAttributeMap,
        biomes: &BiomeAttributes<'_>,
        timelines: &[Timeline],
        can_have_weather: bool,
        clocks: Arc<ClockTotals>,
    ) -> EnvironmentAttributeSystem {
        let mut layers: HashMap<&'static str, Vec<Layer>> = HashMap::new();
        for id in dimension_attributes.key_set() {
            let Some(def) = attribute(id) else { continue };
            let Some(entry) = dimension_attributes.get(id) else {
                continue;
            };
            let Some(modifier) = Modifier::by_name(&entry.modifier) else {
                continue;
            };
            if let Some(arg) = parse_arg(def.kind, modifier, &entry.argument) {
                layers
                    .entry(def.name)
                    .or_default()
                    .push(Layer::Constant(modifier, arg));
            }
        }
        // `addBiomeLayer`: attributes in the order biomes first mention them (`distinct()`); the order only matters per attribute
        let mut per_attribute: Vec<(&'static str, Vec<Option<(Modifier, Arg)>>)> = Vec::new();
        for (index, attributes) in biomes.iter().enumerate() {
            let Some(map) = attributes.and_then(Value::as_object) else {
                continue;
            };
            for (key, entry) in map {
                let Some(def) = Identifier::try_parse(key).and_then(|id| attribute(&id)) else {
                    continue;
                };
                let Some(parsed) = parse_entry(def.kind, entry) else {
                    continue;
                };
                let slot = match per_attribute.iter().position(|(n, _)| *n == def.name) {
                    Some(i) => i,
                    None => {
                        per_attribute.push((def.name, vec![None; biomes.len()]));
                        per_attribute.len() - 1
                    }
                };
                per_attribute[slot].1[index] = Some(parsed);
            }
        }
        for (name, entries) in per_attribute {
            layers
                .entry(name)
                .or_default()
                .push(Layer::Positional(Arc::new(entries)));
        }
        for timeline in timelines {
            for (name, _, track) in &timeline.tracks {
                layers
                    .entry(name)
                    .or_default()
                    .push(Layer::Track(track.clone()));
            }
        }
        if can_have_weather {
            for (name, rain, thunder) in weather_layers() {
                layers
                    .entry(name)
                    .or_default()
                    .push(Layer::Weather { rain, thunder });
            }
        }
        let mut defaults = HashMap::new();
        let mut samplers = HashMap::new();
        for def in ATTRIBUTES {
            let default = default_value(def.name, def.kind);
            if let Some(mut list) = layers.remove(def.name) {
                let mut base = default.clone();
                // `bakeLayerSampler`: leading constant layers fold into the base value
                while let Some(Layer::Constant(modifier, arg)) = list.first() {
                    base = modifier.apply(&base, arg);
                    list.remove(0);
                }
                let is_affected_by_position =
                    list.iter().any(|l| matches!(l, Layer::Positional(_)));
                samplers.insert(
                    def.name,
                    Sampler {
                        range: def.range,
                        base,
                        layers: list,
                        is_affected_by_position,
                        full_resolution_biomes: def.name == "gameplay/natural_mob_spawns",
                        cached: Mutex::new(None),
                    },
                );
            }
            defaults.insert(def.name, default);
        }
        EnvironmentAttributeSystem {
            samplers,
            defaults,
            clocks,
            cache_tick_id: AtomicU64::new(0),
        }
    }

    /// `invalidateTickCache()` (start of `ServerLevel.tick`, after a clock change).
    pub fn invalidate_tick_cache(&self) {
        for sampler in self.samplers.values() {
            *sampler.cached.lock() = None;
        }
        self.cache_tick_id.fetch_add(1, Ordering::Relaxed);
    }

    fn compute(
        &self,
        sampler: &Sampler,
        pos: Option<[f64; 3]>,
        env: &AttributeEnv<'_>,
    ) -> AttrValue {
        let cache_tick_id = self.cache_tick_id.load(Ordering::Relaxed);
        let mut result = sampler.base.clone();
        for layer in &sampler.layers {
            match layer {
                Layer::Constant(m, a) => result = m.apply(&result, a),
                Layer::Track(t) => result = t.apply(&result, cache_tick_id, &self.clocks),
                Layer::Positional(per_biome) => {
                    if let Some(pos) = pos
                        && let Some(biome) = (env.biome_at)(pos, sampler.full_resolution_biomes)
                        && let Some(Some((m, a))) = per_biome.get(biome.0 as usize)
                    {
                        result = m.apply(&result, a);
                    }
                }
                Layer::Weather { rain, thunder } => {
                    let thunder_level = env.thunder_level;
                    let rain_level = env.rain_level - thunder_level;
                    for (level, entry) in [(rain_level, rain), (thunder_level, thunder)] {
                        if let Some((m, a)) = entry
                            && level > 0.0
                        {
                            let to = m.apply(&result, a);
                            result = state_change_lerp(level, &result, &to);
                        }
                    }
                }
            }
        }
        sanitize(sampler.range, result)
    }

    /// `getDimensionValue(attribute)` (cached per tick).
    pub fn get_dimension_value(&self, name: &str, env: &AttributeEnv<'_>) -> AttrValue {
        match self.samplers.get(name) {
            Some(sampler) => {
                if let Some(v) = &*sampler.cached.lock() {
                    return v.clone();
                }
                let value = self.compute(sampler, None, env);
                *sampler.cached.lock() = Some(value.clone());
                value
            }
            None => self.default_value(name),
        }
    }

    /// `getValue(attribute, pos)`.
    pub fn get_value(&self, name: &str, pos: [f64; 3], env: &AttributeEnv<'_>) -> AttrValue {
        match self.samplers.get(name) {
            Some(sampler) if sampler.is_affected_by_position => {
                self.compute(sampler, Some(pos), env)
            }
            Some(_) => self.get_dimension_value(name, env),
            None => self.default_value(name),
        }
    }

    /// `EnvironmentAttribute.defaultValue()`.
    pub fn default_value(&self, name: &str) -> AttrValue {
        self.defaults
            .get(name)
            .unwrap_or_else(|| panic!("unknown environment attribute {name}"))
            .clone()
    }
}

/// `WeatherAttributes.RAIN` / `THUNDER`: per attribute the `(rain, thunder)` entries.
#[allow(clippy::type_complexity)]
fn weather_layers() -> Vec<(
    &'static str,
    Option<(Modifier, Arg)>,
    Option<(Modifier, Arg)>,
)> {
    use AttrValue as V;
    let night_sky_light = color_from_float(1.0, 0.48, 0.48, 1.0);
    let color = |a: f32, r: f32, g: f32, b: f32| color_from_float(a, r, g, b);
    let rgb24 = |c: i32| Arg::Value(V::Rgb(vector3_from_rgb24(c)));
    let argb32 = |c: i32| Arg::Value(V::Argb(vector4_from_argb32(c)));
    let blend = |value: f32, alpha: f32| (Modifier::AlphaBlend, Arg::FloatAlpha { value, alpha });
    let gray = |brightness: f32, factor: f32| {
        (
            Modifier::BlendToGray,
            Arg::BlendToGray { brightness, factor },
        )
    };
    let night_with_alpha =
        |alpha: f32| (night_sky_light & 0xFF_FFFF) | as_8bit_channel(alpha) << 24;
    vec![
        (
            "visual/sky_color",
            Some(gray(0.6, 0.75)),
            Some(gray(0.24, 0.94)),
        ),
        (
            "visual/fog_color",
            Some((Modifier::Multiply, rgb24(color(1.0, 0.5, 0.5, 0.6)))),
            Some((Modifier::Multiply, rgb24(color(1.0, 0.25, 0.25, 0.3)))),
        ),
        (
            "visual/cloud_color",
            Some(gray(0.24, 0.5)),
            Some(gray(0.095, 0.94)),
        ),
        (
            "gameplay/sky_light_level",
            Some(blend(4.0, 0.3125)),
            Some(blend(4.0, 0.52734375)),
        ),
        (
            "visual/sky_light_color",
            Some((Modifier::AlphaBlend, argb32(night_with_alpha(0.3125)))),
            Some((Modifier::AlphaBlend, argb32(night_with_alpha(0.52734375)))),
        ),
        (
            "visual/sky_light_factor",
            Some(blend(0.24, 0.3125)),
            Some(blend(0.24, 0.52734375)),
        ),
        (
            "visual/star_brightness",
            Some((Modifier::Override, Arg::Value(V::Float(0.0)))),
            Some((Modifier::Override, Arg::Value(V::Float(0.0)))),
        ),
        (
            "visual/sunrise_sunset_color",
            Some((Modifier::Multiply, argb32(color(1.0, 0.5, 0.5, 0.6)))),
            Some((Modifier::Multiply, argb32(color(1.0, 0.25, 0.25, 0.3)))),
        ),
        (
            "gameplay/bees_stay_in_hive",
            Some((Modifier::Override, Arg::Value(V::Bool(true)))),
            Some((Modifier::Override, Arg::Value(V::Bool(true)))),
        ),
    ]
}

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

    fn system(ticks: i64) -> (EnvironmentAttributeSystem, Arc<ClockTotals>) {
        let clocks = Arc::new(ClockTotals::default());
        clocks.set(&Identifier::parse("minecraft:overworld").unwrap(), ticks);
        (
            EnvironmentAttributeSystem::build(
                &Default::default(),
                &Vec::new(),
                &[day_timeline()],
                true,
                clocks.clone(),
            ),
            clocks,
        )
    }

    fn level_at(system: &EnvironmentAttributeSystem, rain: f32, thunder: f32) -> f32 {
        system.invalidate_tick_cache();
        let biome = |_: [f64; 3], _: bool| None;
        system
            .get_dimension_value(
                "gameplay/sky_light_level",
                &AttributeEnv {
                    rain_level: rain,
                    thunder_level: thunder,
                    biome_at: &biome,
                },
            )
            .as_float()
    }

    #[test]
    fn sky_light_level_day_night_and_weather() {
        let (day, _) = system(6000);
        assert_eq!(level_at(&day, 0.0, 0.0), 15.0);
        let (night, _) = system(18000);
        assert_eq!(level_at(&night, 0.0, 0.0), 15.0 * 0.26666668);
        // full rain at noon: `alpha_blend(4.0, 0.3125)`
        assert_eq!(level_at(&day, 1.0, 0.0), mth::lerp_f32(0.3125, 15.0, 4.0));
    }

    #[test]
    fn tick_cache_holds_until_invalidated() {
        let (system, clocks) = system(6000);
        let biome = |_: [f64; 3], _: bool| None;
        let env = AttributeEnv {
            rain_level: 0.0,
            thunder_level: 0.0,
            biome_at: &biome,
        };
        assert_eq!(
            system
                .get_dimension_value("gameplay/sky_light_level", &env)
                .as_float(),
            15.0
        );
        clocks.set(&Identifier::parse("minecraft:overworld").unwrap(), 18000);
        assert_eq!(
            system
                .get_dimension_value("gameplay/sky_light_level", &env)
                .as_float(),
            15.0,
            "cached for the tick"
        );
        system.invalidate_tick_cache();
        assert_ne!(
            system
                .get_dimension_value("gameplay/sky_light_level", &env)
                .as_float(),
            15.0
        );
    }
}
