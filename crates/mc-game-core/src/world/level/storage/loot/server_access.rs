//! What loot evaluation reads from outside the level object: the server's random sequences, the scoreboard, command
//! storage, world clocks and the environment attribute system. The server installs the shared handles on the level
//! when it creates it ([`LootServices`] inside `LevelServerState`).

use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::random::{PositionalRandomFactory, RandomSource};
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{BOOL, Codec, DataResult, FLOAT, INT, MapCodec, STRING};
use mc_nbt::CompoundTag;
use parking_lot::Mutex;

use super::context::LootContext;
use crate::registry::typed::attributes::{ATTRIBUTES, AttributeDef, AttributeKind};
use crate::server::level::ServerLevel;
use crate::world::attribute::AttrValue;
use crate::world::level::saveddata::random_sequences::RandomSequences;

/// The shared server data loot reads (`MinecraftServer.getRandomSequence`, `getCommandStorage`).
#[derive(Clone)]
pub struct LootServices {
    /// `MinecraftServer.randomSequences`: the live sequences (the server copies them into its saved data when saving).
    pub random_sequences: Arc<Mutex<RandomSequences>>,
    /// `worldGenSettings.options().seed()`.
    pub world_seed: i64,
    /// `getCommandStorage()` contents by storage id.
    pub command_storage: Arc<dyn Fn(&Identifier) -> CompoundTag + Send + Sync>,
    /// `clockManager().getInstance(clock).totalTicks()`.
    pub clock_total_ticks: Arc<dyn Fn(&Identifier) -> Option<i64> + Send + Sync>,
}

impl Default for LootServices {
    fn default() -> Self {
        LootServices {
            random_sequences: Arc::default(),
            world_seed: 0,
            command_storage: Arc::new(|_| CompoundTag::new()),
            clock_total_ticks: Arc::new(|_| None),
        }
    }
}

/// `RandomSequences.DirtyMarkingRandomSource` over the shared sequences: every draw locks, draws and marks dirty.
struct SequenceRandom {
    sequences: Arc<Mutex<RandomSequences>>,
    key: Identifier,
    world_seed: i64,
}

macro_rules! draw {
    ($self:ident, $m:ident ( $($a:expr),* )) => {
        $self.sequences.lock().get(&$self.key, $self.world_seed).$m($($a),*)
    };
}

impl RandomSource for SequenceRandom {
    fn fork(&mut self) -> Box<dyn RandomSource> {
        draw!(self, fork())
    }
    fn fork_positional(&mut self) -> Box<dyn PositionalRandomFactory> {
        draw!(self, fork_positional())
    }
    fn set_seed(&mut self, seed: i64) {
        draw!(self, set_seed(seed))
    }
    fn next_int(&mut self) -> i32 {
        draw!(self, next_int())
    }
    fn next_int_bounded(&mut self, bound: i32) -> i32 {
        draw!(self, next_int_bounded(bound))
    }
    fn next_long(&mut self) -> i64 {
        draw!(self, next_long())
    }
    fn next_boolean(&mut self) -> bool {
        draw!(self, next_boolean())
    }
    fn next_float(&mut self) -> f32 {
        draw!(self, next_float())
    }
    fn next_double(&mut self) -> f64 {
        draw!(self, next_double())
    }
    fn next_gaussian(&mut self) -> f64 {
        draw!(self, next_gaussian())
    }
}

/// `server.getRandomSequence(key)`.
pub fn random_sequence(level: &ServerLevel, key: &Identifier) -> Box<dyn RandomSource> {
    let services = &level.server_state.loot;
    Box::new(SequenceRandom { sequences: services.random_sequences.clone(), key: key.clone(), world_seed: services.world_seed })
}

/// `level.getRandom()` for a table without a random sequence: vanilla draws from the level's own random, which cannot be
/// borrowed mutably through `&ServerLevel`, so the roll uses a copy of its current state (the level random does not advance).
pub fn level_random(level: &ServerLevel) -> Box<dyn RandomSource> {
    Box::new(level.tick_state.random.clone())
}

/// `scoreboard.getPlayerScoreInfo(holder, objective).value()`.
pub fn score(level: &ServerLevel, holder_name: &str, objective: &str) -> Option<i32> {
    let scoreboard = level.scoreboard();
    scoreboard.get_objective(objective)?;
    scoreboard.get_player_score_info(&crate::world::scores::score::for_name_only(holder_name), objective).map(|s| s.value())
}

/// `server.getCommandStorage().get(id)`.
pub fn command_storage(level: &ServerLevel, id: &Identifier) -> CompoundTag {
    (level.server_state.loot.command_storage)(id)
}

/// `level.clockManager().getInstance(clock).totalTicks()`.
pub fn clock_total_ticks(level: &ServerLevel, clock: &Identifier) -> i64 {
    (level.server_state.loot.clock_total_ticks)(clock).unwrap_or(0)
}

// ---- environment attributes ---------------------------------------------------------------------------------------------

fn attribute_def(id: &Identifier) -> Option<&'static AttributeDef> {
    if id.namespace() != "minecraft" {
        return None;
    }
    ATTRIBUTES.iter().find(|d| d.name == id.path())
}

pub fn environment_attribute_is_positional(id: &Identifier) -> bool {
    attribute_def(id).is_some_and(|d| d.positional)
}

/// `EnvironmentAttributes.CODEC`: the id of an attribute.
pub fn environment_attribute_id_codec() -> Codec<Identifier> {
    IDENTIFIER.flat_xmap(
        |id: Identifier| match attribute_def(&id) {
            Some(_) => DataResult::success(id),
            None => {
                let m = format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:environment_attribute]: {id}");
                DataResult::error(move || m.clone())
            }
        },
        |id: &Identifier| DataResult::success(id.clone()),
    )
}

/// `EnvironmentAttributes.CODEC` restricted to the attributes convertible with `toInt` / `toFloat`.
pub fn environment_attribute_codec(as_float: bool) -> MapCodec<Identifier> {
    IDENTIFIER
        .flat_xmap(
            move |id: Identifier| match attribute_def(&id) {
                None => {
                    let m = format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:environment_attribute]: {id}");
                    DataResult::error(move || m.clone())
                }
                Some(d) => {
                    let convertible = if as_float {
                        matches!(d.kind, AttributeKind::Float | AttributeKind::AngleDegrees | AttributeKind::Integer)
                    } else {
                        d.kind == AttributeKind::Integer
                    };
                    if convertible {
                        DataResult::success(id)
                    } else {
                        let what = if as_float { "float" } else { "an integer" };
                        let m = format!("minecraft:{} cannot be converted to {what}", d.name);
                        let m = m.replace("to float", "to a float");
                        DataResult::error(move || m.clone())
                    }
                }
            },
            |id: &Identifier| DataResult::success(id.clone()),
        )
        .field_of("attribute")
}

/// `environmentAttributes().getValue(context, attribute)`: positional attributes read at `ORIGIN` (the default without
/// one), the others at the dimension.
fn attribute_value(context: &LootContext, id: &Identifier) -> Option<AttrValue> {
    let def = attribute_def(id)?;
    let level = context.get_level();
    if def.positional {
        Some(match context.params().origin {
            Some(origin) => level.environment_value(def.name, &origin),
            None => level.tick_state.attributes.default_value(def.name),
        })
    } else {
        Some(level.environment_dimension_value(def.name))
    }
}

pub fn environment_attribute_float(context: &LootContext, id: &Identifier) -> Option<f32> {
    attribute_value(context, id)?.to_float()
}

pub fn environment_attribute_int(context: &LootContext, id: &Identifier) -> Option<i32> {
    attribute_value(context, id)?.to_int()
}

/// `EnvironmentAttributeCheck`'s `value` codec for an attribute (`attribute.valueCodec()`).
pub fn environment_attribute_value_codec(id: &Identifier) -> Option<Codec<AttrValue>> {
    let def = attribute_def(id)?;
    Some(match def.kind {
        AttributeKind::Float | AttributeKind::AngleDegrees => FLOAT.xmap(AttrValue::Float, |v: &AttrValue| if let AttrValue::Float(f) = v { *f } else { 0.0 }),
        AttributeKind::Boolean => BOOL.xmap(AttrValue::Bool, |v: &AttrValue| matches!(v, AttrValue::Bool(true))),
        AttributeKind::Integer => INT.xmap(AttrValue::Int, |v: &AttrValue| v.to_int().unwrap_or(0)),
        AttributeKind::MoonPhase => STRING.flat_xmap(
            |name: String| match crate::world::attribute::value::MOON_PHASES.iter().position(|p| *p == name) {
                Some(i) => DataResult::success(AttrValue::MoonPhase(i as u8)),
                None => DataResult::error(move || format!("Unknown moon phase: {name}")),
            },
            |v: &AttrValue| match v {
                AttrValue::MoonPhase(i) => DataResult::success(crate::world::attribute::value::MOON_PHASES[usize::from(*i)].to_owned()),
                _ => DataResult::error(|| "not a moon phase".to_owned()),
            },
        ),
        _ => return None,
    })
}

/// `EnvironmentAttributeCheck.test`: the sampled value equals `value`.
pub fn environment_attribute_matches(context: &LootContext, id: &Identifier, value: &AttrValue, _origin: Option<Vec3>) -> bool {
    attribute_value(context, id).is_some_and(|actual| match (actual, value) {
        (AttrValue::Float(a), AttrValue::Float(b)) => a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()),
        (a, b) => a == *b,
    })
}
