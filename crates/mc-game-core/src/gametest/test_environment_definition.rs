//! `TestEnvironmentDefinition` (`test_environment` registry): `all_of`, `clock_time`, `difficulty`, `function`,
//! `game_rules`, `timeline_attributes` and `weather`, with `setup` / `teardown` (`Activation`).

use crate::registry::codec::{RegistryDecodeContext, json_to_nbt};
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::{RegistryAccess, keys};
use crate::server::MinecraftServer;
use crate::server::level_tick_env::ServerTickEnv;
use crate::world::clock::ClockInstance;
use crate::world::level::DimensionKey;
use crate::world::level::gamerules::GameRuleMap;
use mc_core::ident::Identifier;
use mc_world::level_storage::Difficulty;
use serde_json::Value;
use std::sync::Arc;

/// `Holder<TestEnvironmentDefinition<?>>` as `RegistryCodecs.holder` decodes it: a registry reference or a direct value.
#[derive(Clone)]
pub enum EnvironmentRef {
    Registered(Identifier),
    Direct(Arc<TestEnvironmentDefinition>),
}

impl EnvironmentRef {
    /// `Holder.getRegisteredName()` (`Holder.Direct` answers `[unregistered]`).
    pub fn registered_name(&self) -> String {
        match self {
            Self::Registered(id) => id.to_string(),
            Self::Direct(_) => "[unregistered]".to_owned(),
        }
    }

    /// `Holder.value()`.
    pub fn value(&self, registries: &RegistryAccess) -> Option<Arc<TestEnvironmentDefinition>> {
        match self {
            Self::Direct(def) => Some(def.clone()),
            Self::Registered(id) => {
                let entry = registries.registry(keys::TEST_ENVIRONMENT)?.entry_by_key(id)?;
                entry.value.clone()?.downcast::<TestEnvironmentDefinition>().ok()
            }
        }
    }
}

impl PartialEq for EnvironmentRef {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Registered(a), Self::Registered(b)) => a == b,
            (Self::Direct(a), Self::Direct(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// `TestEnvironmentDefinition.Weather.Type`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WeatherType {
    Clear,
    Rain,
    Thunder,
}

impl WeatherType {
    fn by_name(name: &str) -> Option<WeatherType> {
        match name {
            "clear" => Some(Self::Clear),
            "rain" => Some(Self::Rain),
            "thunder" => Some(Self::Thunder),
            _ => None,
        }
    }

    /// `apply(level)`: `server.setWeatherParameters(clearTime, rainTime, raining, thundering)`.
    fn apply(self, server: &mut MinecraftServer) {
        let (clear_time, rain_time, raining, thundering) = match self {
            Self::Clear => (100000, 0, false, false),
            Self::Rain => (0, 100000, true, false),
            Self::Thunder => (0, 100000, true, true),
        };
        server.set_weather_parameters(clear_time, rain_time, raining, thundering);
    }
}

pub enum TestEnvironmentDefinition {
    AllOf(Vec<EnvironmentRef>),
    ClockTime { clock: Identifier, time: i32 },
    SetDifficulty(Difficulty),
    Functions { setup_function: Option<Identifier>, teardown_function: Option<Identifier> },
    SetGameRules(GameRuleMap),
    Timelines(Vec<Identifier>),
    Weather(WeatherType),
}

/// The state `setup` returns for `teardown` (`SavedDataType`).
enum Saved {
    Activations(Vec<Activation>),
    ClockTicks(i64),
    Difficulty(Difficulty),
    Unit,
    GameRules(GameRuleMap),
    Attributes(crate::world::attribute::EnvironmentAttributeSystem),
    Weather(WeatherType),
}

/// `TestEnvironmentDefinition.Activation<T>`.
pub struct Activation {
    value: Saved,
    definition: Arc<TestEnvironmentDefinition>,
    level: DimensionKey,
}

impl Activation {
    /// `activate(environment, level)`.
    pub fn activate(server: &mut MinecraftServer, definition: Arc<TestEnvironmentDefinition>, level: &DimensionKey) -> Activation {
        let value = definition.setup(server, level);
        Activation { value, definition, level: level.clone() }
    }

    /// `teardown()`.
    pub fn teardown(self, server: &mut MinecraftServer) {
        let Activation { value, definition, level } = self;
        definition.teardown(server, &level, value);
    }
}

impl TestEnvironmentDefinition {
    fn setup(&self, server: &mut MinecraftServer, level: &DimensionKey) -> Saved {
        match self {
            Self::AllOf(definitions) => {
                let registries = server.registries.composite_access().clone();
                let activations = definitions
                    .iter()
                    .filter_map(|b| b.value(&registries))
                    .map(|def| Activation::activate(server, def, level))
                    .collect();
                Saved::Activations(activations)
            }
            Self::ClockTime { clock, time } => {
                let previous = server.clock_manager().get_instance(clock).total_ticks();
                server.with_clock_manager(|clocks, host| clocks.set_total_ticks(host, clock, i64::from(*time)));
                Saved::ClockTicks(previous)
            }
            Self::SetDifficulty(difficulty) => {
                let old = server.world_data.lock().settings.difficulty_settings.difficulty;
                server.set_difficulty(*difficulty, true);
                Saved::Difficulty(old)
            }
            Self::Functions { setup_function, .. } => {
                if let Some(id) = setup_function {
                    run_function(server, level, id);
                }
                Saved::Unit
            }
            Self::SetGameRules(map) => {
                let mut original = GameRuleMap::of_empty();
                for rule in map.key_set() {
                    original.set_value(rule, server.game_rules().get_value(rule));
                }
                set_game_rules(server, map);
                Saved::GameRules(original)
            }
            Self::Timelines(timelines) => Saved::Attributes(set_timelines(server, level, timelines)),
            Self::Weather(weather) => {
                let previous = {
                    let lvl = server.level(level).expect("test level");
                    if lvl.is_thundering() {
                        WeatherType::Thunder
                    } else if lvl.is_raining() {
                        WeatherType::Rain
                    } else {
                        WeatherType::Clear
                    }
                };
                weather.apply(server);
                Saved::Weather(previous)
            }
        }
    }

    fn teardown(&self, server: &mut MinecraftServer, level: &DimensionKey, saved: Saved) {
        match (self, saved) {
            (Self::AllOf(_), Saved::Activations(activations)) => {
                for activation in activations.into_iter().rev() {
                    activation.teardown(server);
                }
            }
            (Self::ClockTime { clock, .. }, Saved::ClockTicks(previous)) => {
                server.with_clock_manager(|clocks, host| clocks.set_total_ticks(host, clock, previous));
            }
            (Self::SetDifficulty(_), Saved::Difficulty(old)) => server.set_difficulty(old, true),
            (Self::Functions { teardown_function, .. }, Saved::Unit) => {
                if let Some(id) = teardown_function {
                    run_function(server, level, id);
                }
            }
            (Self::SetGameRules(_), Saved::GameRules(original)) => set_game_rules(server, &original),
            (Self::Timelines(_), Saved::Attributes(previous)) => {
                if let Some(lvl) = server.level_mut(level) {
                    lvl.tick_state.attributes = previous;
                }
            }
            (Self::Weather(_), Saved::Weather(previous)) => {
                server.with_level(level, |server, lvl| {
                    let mut env = ServerTickEnv { server, dimension: level.clone() };
                    lvl.reset_weather_cycle(&mut env);
                });
                previous.apply(server);
            }
            _ => unreachable!("an activation is always torn down by the definition that set it up"),
        }
    }
}

/// `gameRules.setAll(map, server)`.
fn set_game_rules(server: &mut MinecraftServer, map: &GameRuleMap) {
    for rule in map.key_set() {
        server.set_game_rule_value(rule, map.get_value(rule).expect("key of map"));
    }
}

/// `Functions.run(level, functionId)`.
fn run_function(server: &mut MinecraftServer, level: &DimensionKey, id: &Identifier) {
    match server.functions().get(id) {
        Some(function) => {
            let source = server
                .create_command_source_stack()
                .with_permission(crate::server::permissions::LevelBasedPermissionSet::GAMEMASTER.into())
                .with_suppressed_output()
                .with_level(level.clone(), 1.0);
            server.execute_function(&function, source);
        }
        None => log::error!("Test Batch failed for non-existent function {id}"),
    }
}

/// `Timelines.setup(level)`: `level.setEnvironmentAttributes(builder().addDefaultLayers(level)` + the timelines`.build())`,
/// returning the previous system.
fn set_timelines(server: &mut MinecraftServer, level: &DimensionKey, timelines: &[Identifier]) -> crate::world::attribute::EnvironmentAttributeSystem {
    let system = server.build_attribute_system_for(level, timelines);
    let lvl = server.level_mut(level).expect("test level");
    std::mem::replace(&mut lvl.tick_state.attributes, system)
}

// ---- codec ----

pub fn decode_environment_ref(value: &Value, ctx: &RegistryDecodeContext) -> Result<EnvironmentRef, String> {
    match value {
        Value::String(s) => {
            let id = Identifier::read(s)?;
            ctx.check_holder(keys::TEST_ENVIRONMENT, &id)?;
            Ok(EnvironmentRef::Registered(id))
        }
        other => Ok(EnvironmentRef::Direct(Arc::new(decode_definition(other, ctx)?))),
    }
}

fn required<'a>(object: &'a Value, key: &str) -> Result<&'a Value, String> {
    object.get(key).ok_or_else(|| format!("No key {key} in MapLike[{}]", object))
}

fn non_negative_int(value: &Value) -> Result<i32, String> {
    let n = value.as_f64().ok_or_else(|| format!("Not a number: {value}"))?;
    let n = n as i32;
    if n < 0 { Err(format!("Value must be non-negative: {n}")) } else { Ok(n) }
}

/// `TestEnvironmentDefinition.DIRECT_CODEC`.
pub fn decode_definition(json: &Value, ctx: &RegistryDecodeContext) -> Result<TestEnvironmentDefinition, String> {
    let object = json.as_object().ok_or_else(|| format!("Not a JSON object: {json}"))?;
    let ty = required(json, "type")?.as_str().ok_or_else(|| "Not a string: type".to_owned())?;
    let ty = Identifier::read(ty)?;
    Ok(match ty.to_string().as_str() {
        "minecraft:all_of" => {
            let list = required(json, "definitions")?.as_array().ok_or_else(|| "Not a list: definitions".to_owned())?;
            TestEnvironmentDefinition::AllOf(list.iter().map(|d| decode_environment_ref(d, ctx)).collect::<Result<_, _>>()?)
        }
        "minecraft:clock_time" => {
            let clock = Identifier::read(required(json, "clock")?.as_str().ok_or_else(|| "Not a string: clock".to_owned())?)?;
            ctx.check_holder(keys::WORLD_CLOCK, &clock)?;
            TestEnvironmentDefinition::ClockTime { clock, time: non_negative_int(required(json, "time")?)? }
        }
        "minecraft:difficulty" => {
            let name = required(json, "difficulty")?.as_str().ok_or_else(|| "Not a string: difficulty".to_owned())?;
            let difficulty = match name {
                "peaceful" => Difficulty::Peaceful,
                "easy" => Difficulty::Easy,
                "normal" => Difficulty::Normal,
                "hard" => Difficulty::Hard,
                other => return Err(format!("Unknown enum value {other}")),
            };
            TestEnvironmentDefinition::SetDifficulty(difficulty)
        }
        "minecraft:function" => {
            let read = |key: &str| -> Result<Option<Identifier>, String> {
                match object.get(key) {
                    None => Ok(None),
                    Some(v) => Identifier::read(v.as_str().ok_or_else(|| format!("Not a string: {key}"))?).map(Some),
                }
            };
            TestEnvironmentDefinition::Functions { setup_function: read("setup")?, teardown_function: read("teardown")? }
        }
        "minecraft:game_rules" => {
            let rules = required(json, "rules")?;
            if !rules.is_object() {
                return Err(format!("Not a map: {rules}"));
            }
            let map = GameRuleMap::decode(&json_to_nbt(rules)).map_err(|p| p.error)?;
            TestEnvironmentDefinition::SetGameRules(map)
        }
        "minecraft:timeline_attributes" => {
            let list = required(json, "timelines")?.as_array().ok_or_else(|| "Not a list: timelines".to_owned())?;
            let mut timelines = Vec::with_capacity(list.len());
            for t in list {
                let id = Identifier::read(t.as_str().ok_or_else(|| format!("Not a string: {t}"))?)?;
                ctx.check_holder(keys::TIMELINE, &id)?;
                timelines.push(id);
            }
            TestEnvironmentDefinition::Timelines(timelines)
        }
        "minecraft:weather" => {
            let name = required(json, "weather")?.as_str().ok_or_else(|| "Not a string: weather".to_owned())?;
            TestEnvironmentDefinition::Weather(WeatherType::by_name(name).ok_or_else(|| format!("Unknown enum value {name}"))?)
        }
        other => return Err(format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:test_environment_definition_type]: {other}")),
    })
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    Ok(DecodedEntry::new(decode_definition(json, ctx)?))
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::TEST_ENVIRONMENT, decode };

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::codec::{Lookup, RegistryLookup};
    use serde_json::json;

    struct All;

    impl RegistryLookup for All {
        fn contains(&self, _registry: &str, _id: &Identifier) -> Lookup {
            Lookup::Yes
        }
        fn tag_exists(&self, _registry: &str, _tag: &Identifier) -> Lookup {
            Lookup::Yes
        }
    }

    #[test]
    fn decodes_every_definition_type() {
        let ctx = RegistryDecodeContext::new(&All);
        let default = decode_definition(&json!({"type":"minecraft:all_of","definitions":[]}), &ctx).unwrap();
        assert!(matches!(default, TestEnvironmentDefinition::AllOf(ref d) if d.is_empty()));
        let nested = decode_definition(
            &json!({"type":"minecraft:all_of","definitions":["minecraft:default",{"type":"minecraft:weather","weather":"thunder"},{"type":"minecraft:clock_time","clock":"minecraft:overworld","time":6000}]}),
            &ctx,
        )
        .unwrap();
        match nested {
            TestEnvironmentDefinition::AllOf(d) => {
                assert_eq!(d.len(), 3);
                assert_eq!(d[0].registered_name(), "minecraft:default");
                assert_eq!(d[1].registered_name(), "[unregistered]");
            }
            _ => panic!("all_of"),
        }
        assert!(matches!(decode_definition(&json!({"type":"minecraft:difficulty","difficulty":"hard"}), &ctx), Ok(TestEnvironmentDefinition::SetDifficulty(Difficulty::Hard))));
        assert!(matches!(
            decode_definition(&json!({"type":"minecraft:function","setup":"a:b"}), &ctx),
            Ok(TestEnvironmentDefinition::Functions { setup_function: Some(_), teardown_function: None })
        ));
        assert!(matches!(decode_definition(&json!({"type":"minecraft:game_rules","rules":{"minecraft:pvp":false}}), &ctx), Ok(TestEnvironmentDefinition::SetGameRules(_))));
        assert!(decode_definition(&json!({"type":"minecraft:clock_time","clock":"minecraft:overworld","time":-1}), &ctx).is_err());
        assert!(decode_definition(&json!({"type":"minecraft:weather","weather":"hail"}), &ctx).is_err());
    }
}
