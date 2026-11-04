//! `GameRules`: the rule constants and the feature-filtered map a level uses.

use mc_nbt::Tag;
use mc_world::level_storage::FeatureFlagSet;

use super::{AnyGameRule, GameRule, GameRuleMap, GameRuleTypeVisitor, GameRuleValue, GameRuleValueType};
use crate::util::codec::{DecodeResult, map_decoded};
use crate::world::level::saveddata::{SavedData, SavedDataType};

/// The side effects vanilla runs in `MinecraftServer.onGameRuleChanged(rule, value)`; the server implements it.
pub trait GameRuleListener {
    fn on_game_rule_changed(&mut self, rule: AnyGameRule, value: GameRuleValue);
}

macro_rules! rules {
    ($($name:ident: $ty:ty),* $(,)?) => {
        $(pub const $name: GameRule<$ty> = GameRule::new(mc_data::GameRule::$name);)*
    };
}

/// `GameRules`: a [`GameRuleMap`] restricted to the rules enabled by the world's feature flags. It is itself the
/// saved data of `game_rules.dat` (see [`GameRules::type_`]).
#[derive(Debug, Clone)]
pub struct GameRules {
    rules: GameRuleMap,
}

impl SavedData for GameRules {
    fn is_dirty(&self) -> bool {
        self.rules.is_dirty()
    }
    fn set_dirty_to(&mut self, dirty: bool) {
        self.rules.set_dirty_to(dirty);
    }
}

impl GameRules {
    rules! {
        ADVANCE_TIME: bool, ADVANCE_WEATHER: bool, ALLOW_ENTERING_NETHER_USING_PORTALS: bool, BLOCK_DROPS: bool,
        BLOCK_EXPLOSION_DROP_DECAY: bool, COMMAND_BLOCKS_WORK: bool, COMMAND_BLOCK_OUTPUT: bool, DROWNING_DAMAGE: bool,
        ELYTRA_MOVEMENT_CHECK: bool, ENDER_PEARLS_VANISH_ON_DEATH: bool, ENTITY_DROPS: bool, FALL_DAMAGE: bool,
        FIRE_DAMAGE: bool, FIRE_SPREAD_RADIUS_AROUND_PLAYER: i32, FORGIVE_DEAD_PLAYERS: bool, FREEZE_DAMAGE: bool,
        GLOBAL_SOUND_EVENTS: bool, IMMEDIATE_RESPAWN: bool, KEEP_INVENTORY: bool, LAVA_SOURCE_CONVERSION: bool,
        LIMITED_CRAFTING: bool, LOCATOR_BAR: bool, LOG_ADMIN_COMMANDS: bool, MAX_BLOCK_MODIFICATIONS: i32,
        MAX_COMMAND_FORKS: i32, MAX_COMMAND_SEQUENCE_LENGTH: i32, MAX_ENTITY_CRAMMING: i32, MAX_MINECART_SPEED: i32,
        MAX_SNOW_ACCUMULATION_HEIGHT: i32, MOB_DROPS: bool, MOB_EXPLOSION_DROP_DECAY: bool, MOB_GRIEFING: bool,
        NATURAL_HEALTH_REGENERATION: bool, PLAYER_MOVEMENT_CHECK: bool, PLAYERS_NETHER_PORTAL_CREATIVE_DELAY: i32,
        PLAYERS_NETHER_PORTAL_DEFAULT_DELAY: i32, PLAYERS_SLEEPING_PERCENTAGE: i32, PROJECTILES_CAN_BREAK_BLOCKS: bool,
        PVP: bool, RAIDS: bool, RANDOM_TICK_SPEED: i32, REDUCED_DEBUG_INFO: bool, RESPAWN_RADIUS: i32,
        SEND_COMMAND_FEEDBACK: bool, SHOW_ADVANCEMENT_MESSAGES: bool, SHOW_DEATH_MESSAGES: bool,
        SPAWNER_BLOCKS_WORK: bool, SPAWN_MOBS: bool, SPAWN_MONSTERS: bool, SPAWN_PATROLS: bool, SPAWN_PHANTOMS: bool,
        SPAWN_WANDERING_TRADERS: bool, SPAWN_WARDENS: bool, SPECTATORS_GENERATE_CHUNKS: bool, SPREAD_VINES: bool,
        TNT_EXPLODES: bool, TNT_EXPLOSION_DROP_DECAY: bool, UNIVERSAL_ANGER: bool, WATER_SOURCE_CONVERSION: bool,
    }

    /// `new GameRules(enabledFeatures)`: every enabled rule at its default.
    pub fn new(enabled_features: FeatureFlagSet) -> GameRules {
        GameRules { rules: GameRuleMap::of(AnyGameRule::all().filter(|r| r.is_enabled(enabled_features))) }
    }

    /// `new GameRules(enabledFeatures, map)`: missing enabled rules are reset to their default, rules whose features
    /// are off are dropped.
    pub fn from_map(enabled_features: FeatureFlagSet, mut map: GameRuleMap) -> GameRules {
        for rule in AnyGameRule::all() {
            if rule.is_enabled(enabled_features) {
                if !map.has(rule) {
                    map.reset(rule);
                }
            } else if map.has(rule) {
                map.remove(rule);
            }
        }
        GameRules { rules: map }
    }

    /// `new GameRules(List<GameRule<?>>)`.
    pub fn of_rules(rules: impl IntoIterator<Item = AnyGameRule>) -> GameRules {
        GameRules { rules: GameRuleMap::of(rules) }
    }

    /// `availableRules()`.
    pub fn available_rules(&self) -> impl Iterator<Item = AnyGameRule> + '_ {
        self.rules.key_set()
    }

    /// `get`: panics for a rule that is not available (`IllegalArgumentException`).
    pub fn get<T: GameRuleValueType>(&self, rule: GameRule<T>) -> T {
        self.rules.get(rule).unwrap_or_else(|| panic!("Tried to access invalid game rule {}", rule.get_identifier()))
    }

    pub fn get_value(&self, rule: AnyGameRule) -> GameRuleValue {
        self.rules.get_value(rule).unwrap_or_else(|| panic!("Tried to access invalid game rule {}", rule.get_identifier()))
    }

    /// `set(rule, value, server)`: logs and ignores rules that are not available; notifies `server` of the change.
    pub fn set<T: GameRuleValueType>(&mut self, rule: GameRule<T>, value: T, server: Option<&mut dyn GameRuleListener>) {
        self.set_value(rule.erase(), value.into_value(), server);
    }

    pub fn set_value(&mut self, rule: AnyGameRule, value: GameRuleValue, server: Option<&mut dyn GameRuleListener>) {
        if self.try_set_value(rule, value)
            && let Some(server) = server
        {
            server.on_game_rule_changed(rule, value);
        }
    }

    /// The mutation part of `set`, without the server notification (for callers that hold the rules inside the
    /// object that would be notified): returns `false` (after logging) when the rule is not available.
    pub fn try_set_value(&mut self, rule: AnyGameRule, value: GameRuleValue) -> bool {
        if !self.rules.has(rule) {
            log::warn!("Tried to set invalid game rule '{}' to value '{}'", rule.get_identifier(), rule.serialize(value));
            return false;
        }
        self.rules.set_value(rule, value);
        true
    }

    /// `copy(enabledFeatures)`.
    pub fn copy(&self, enabled_features: FeatureFlagSet) -> GameRules {
        GameRules::from_map(enabled_features, GameRuleMap::copy_of(&self.rules))
    }

    /// `setAll(GameRules, server)`.
    pub fn set_all(&mut self, other: &GameRules, server: Option<&mut dyn GameRuleListener>) {
        self.set_all_map(&other.rules, server);
    }

    /// `setAll(GameRuleMap, server)`.
    pub fn set_all_map(&mut self, map: &GameRuleMap, server: Option<&mut dyn GameRuleListener>) {
        let mut server = server;
        for rule in map.key_set() {
            let value = map.get_value(rule).expect("key of map");
            match &mut server {
                Some(s) => self.set_value(rule, value, Some(&mut **s)),
                None => self.set_value(rule, value, None),
            }
        }
    }

    /// `visitGameRuleTypes`.
    pub fn visit_game_rule_types(&self, visitor: &mut dyn GameRuleTypeVisitor) {
        for rule in self.rules.key_set() {
            visitor.visit(rule);
            rule.call_visitor(visitor);
        }
    }

    /// `getAsString`.
    pub fn get_as_string(&self, rule: AnyGameRule) -> String {
        rule.serialize(self.get_value(rule))
    }

    /// The underlying map (what `game_rules.dat` stores).
    pub fn map(&self) -> &GameRuleMap {
        &self.rules
    }

    /// `GameRules.codec(enabledFeatures)` decode.
    pub fn decode(enabled_features: FeatureFlagSet, tag: &Tag) -> DecodeResult<GameRules> {
        map_decoded(GameRuleMap::decode(tag), |map| GameRules::from_map(enabled_features, map))
    }

    /// `GameRules.codec(enabledFeatures)` encode.
    pub fn encode(&self) -> Tag {
        self.rules.encode()
    }

    /// `GameRuleMap.TYPE` with the `GameRules` wrapper (`MinecraftServer`: `new GameRules(features,
    /// storage.computeIfAbsent(GameRuleMap.TYPE))`). A fresh file starts empty and is filled with the defaults.
    pub fn type_(enabled_features: FeatureFlagSet) -> SavedDataType<GameRules> {
        SavedDataType::new(
            GameRuleMap::type_().id,
            move || GameRules::from_map(enabled_features, GameRuleMap::of_empty()),
            move |tag| GameRules::decode(enabled_features, tag),
            |rules| Ok(rules.encode()),
        )
    }
}
