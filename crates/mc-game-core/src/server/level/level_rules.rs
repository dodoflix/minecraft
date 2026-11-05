//! The game rules gameplay code reads through `ServerLevel` (`level.getGameRules().get(GameRules.X)`).
//!
//! [`LiveRules`] is a handle shared by the server's `GameRules` and every level: `GameRules` refreshes it the moment a
//! rule changes, so `ServerLevel::<rule name>()` reads the current value at the point of use like vanilla's
//! `level.getGameRules().get(...)` (e.g. `/gamerule keepInventory true` then `/kill` in the same tick).
//! A rule that is disabled by the world's feature flags reads as its default (vanilla throws for such a rule, which
//! no caller can observe).

use super::ServerLevel;
use crate::world::level::gamerules::{AnyGameRule, GameRuleMap, GameRules};
use parking_lot::RwLock;
use std::sync::Arc;
use mc_world::level_storage::FeatureFlagSet;

macro_rules! level_rules {
    ($($field:ident: $ty:ty = $rule:ident),* $(,)?) => {
        /// A snapshot of the game rules `ServerLevel` and the entity code read.
        #[derive(Clone, Copy, Debug)]
        pub struct LevelRules {
            $(#[doc = concat!("`", stringify!($rule), "`.")] pub $field: $ty,)*
        }

        impl LevelRules {
            /// Reads every rule from `rules` (missing = disabled rule = its default).
            pub fn from_game_rules(rules: &GameRules) -> Self {
                Self::from_map(rules.map())
            }

            /// Reads every rule from `map` (missing = disabled rule = its default).
            pub fn from_map(map: &GameRuleMap) -> Self {
                Self { $($field: map.get(GameRules::$rule).unwrap_or_else(|| GameRules::$rule.default_value()),)* }
            }
        }

        impl Default for LevelRules {
            /// Vanilla defaults.
            fn default() -> Self {
                Self::from_map(&GameRuleMap::of(AnyGameRule::all().filter(|r| r.is_enabled(FeatureFlagSet::DEFAULT_FLAGS))))
            }
        }

        impl ServerLevel {
            $(#[doc = concat!("`getGameRules().get(GameRules.", stringify!($rule), ")`.")]
            pub fn $field(&self) -> $ty {
                self.server_state.rules.get().$field
            })*
        }
    };
}

/// The shared, always-current rule values (see the module doc).
#[derive(Clone, Debug, Default)]
pub struct LiveRules(Arc<RwLock<LevelRules>>);

impl LiveRules {
    /// The current values.
    pub fn get(&self) -> LevelRules {
        *self.0.read()
    }

    /// A handle holding the values of `map`.
    pub fn of_map(map: &GameRuleMap) -> Self {
        Self(Arc::new(RwLock::new(LevelRules::from_map(map))))
    }

    /// Re-reads every rule from `rules` (called by `GameRules` on each change).
    pub fn refresh(&self, rules: &GameRules) {
        *self.0.write() = LevelRules::from_game_rules(rules);
    }

    /// Mutates the values directly (tests that run a level without a server).
    pub fn update(&self, f: impl FnOnce(&mut LevelRules)) {
        f(&mut self.0.write());
    }
}

level_rules! {
    advance_time: bool = ADVANCE_TIME,
    advance_weather: bool = ADVANCE_WEATHER,
    allow_entering_nether_using_portals: bool = ALLOW_ENTERING_NETHER_USING_PORTALS,
    block_drops: bool = BLOCK_DROPS,
    block_explosion_drop_decay: bool = BLOCK_EXPLOSION_DROP_DECAY,
    command_blocks_work: bool = COMMAND_BLOCKS_WORK,
    drowning_damage: bool = DROWNING_DAMAGE,
    elytra_movement_check: bool = ELYTRA_MOVEMENT_CHECK,
    ender_pearls_vanish_on_death: bool = ENDER_PEARLS_VANISH_ON_DEATH,
    entity_drops: bool = ENTITY_DROPS,
    fall_damage: bool = FALL_DAMAGE,
    fire_damage: bool = FIRE_DAMAGE,
    fire_spread_radius_around_player: i32 = FIRE_SPREAD_RADIUS_AROUND_PLAYER,
    forgive_dead_players: bool = FORGIVE_DEAD_PLAYERS,
    freeze_damage: bool = FREEZE_DAMAGE,
    immediate_respawn: bool = IMMEDIATE_RESPAWN,
    keep_inventory: bool = KEEP_INVENTORY,
    lava_source_conversion: bool = LAVA_SOURCE_CONVERSION,
    limited_crafting: bool = LIMITED_CRAFTING,
    locator_bar: bool = LOCATOR_BAR,
    max_entity_cramming: i32 = MAX_ENTITY_CRAMMING,
    max_snow_accumulation_height: i32 = MAX_SNOW_ACCUMULATION_HEIGHT,
    mob_drops: bool = MOB_DROPS,
    mob_explosion_drop_decay: bool = MOB_EXPLOSION_DROP_DECAY,
    mob_griefing: bool = MOB_GRIEFING,
    natural_health_regeneration: bool = NATURAL_HEALTH_REGENERATION,
    player_movement_check: bool = PLAYER_MOVEMENT_CHECK,
    players_nether_portal_creative_delay: i32 = PLAYERS_NETHER_PORTAL_CREATIVE_DELAY,
    players_nether_portal_default_delay: i32 = PLAYERS_NETHER_PORTAL_DEFAULT_DELAY,
    players_sleeping_percentage: i32 = PLAYERS_SLEEPING_PERCENTAGE,
    projectiles_can_break_blocks: bool = PROJECTILES_CAN_BREAK_BLOCKS,
    pvp: bool = PVP,
    raids: bool = RAIDS,
    random_tick_speed: i32 = RANDOM_TICK_SPEED,
    reduced_debug_info: bool = REDUCED_DEBUG_INFO,
    show_advancement_messages: bool = SHOW_ADVANCEMENT_MESSAGES,
    show_death_messages: bool = SHOW_DEATH_MESSAGES,
    spawn_mobs: bool = SPAWN_MOBS,
    spawn_monsters: bool = SPAWN_MONSTERS,
    spawn_patrols: bool = SPAWN_PATROLS,
    spawn_phantoms: bool = SPAWN_PHANTOMS,
    spawn_wandering_traders: bool = SPAWN_WANDERING_TRADERS,
    spawn_wardens: bool = SPAWN_WARDENS,
    spawner_blocks_work: bool = SPAWNER_BLOCKS_WORK,
    spread_vines: bool = SPREAD_VINES,
    tnt_explodes: bool = TNT_EXPLODES,
    tnt_explosion_drop_decay: bool = TNT_EXPLOSION_DROP_DECAY,
    universal_anger: bool = UNIVERSAL_ANGER,
    water_source_conversion: bool = WATER_SOURCE_CONVERSION,
}
