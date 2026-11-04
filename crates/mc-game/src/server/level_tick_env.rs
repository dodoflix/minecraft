//! `LevelTickEnv` for the server: what `ServerLevel.tick` reads from `MinecraftServer` (tick rate manager, game rules,
//! weather saved data, difficulty, clocks, player list).
//!
//! Not modelled yet (their owners have not landed): raids, the scheduled-events timer queue (needs the function manager) and the
//! environment attribute system behind `sky_light_level` / `moon_brightness` (the overworld day/night cycle is read from
//! the overworld clock with the vanilla day length instead).

use super::MinecraftServer;
use crate::server::level::ServerLevel;
use crate::server::level::level_tick::{LevelRules, LevelTickEnv, WeatherState};
use crate::world::entity::Entity;
use crate::world::level::gamerules::GameRules;
use crate::world::level::saveddata::weather_data::WeatherData;
use crate::world::level::DimensionKey;
use mc_core::pos::BlockPos;
use mc_protocol::packets::play::ClientboundGameEventPacket;
use mc_world::level_storage::Difficulty;

/// A borrow of the server for the duration of one level's tick (the level itself is taken out of the server).
pub struct ServerTickEnv<'a> {
    pub server: &'a mut MinecraftServer,
    /// The dimension of the level being ticked.
    pub dimension: DimensionKey,
}

impl LevelTickEnv for ServerTickEnv<'_> {
    fn runs_normally(&self) -> bool {
        self.server.tick_rate_manager.runs_normally()
    }

    fn is_entity_frozen(&self, entity: &dyn Entity) -> bool {
        // `countPlayerPassengers() <= 0`: any passenger might be a player, the entity is only frozen when it has none.
        self.server.tick_rate_manager.is_entity_frozen(entity.is_player(), i32::from(!entity.base().passengers.is_empty()))
    }

    fn rules(&self) -> LevelRules {
        let rules = self.server.game_rules_ref();
        LevelRules {
            players_sleeping_percentage: rules.get(GameRules::PLAYERS_SLEEPING_PERCENTAGE),
            advance_time: rules.get(GameRules::ADVANCE_TIME),
            advance_weather: rules.get(GameRules::ADVANCE_WEATHER),
            spawn_mobs: rules.get(GameRules::SPAWN_MOBS),
            max_snow_accumulation_height: rules.get(GameRules::MAX_SNOW_ACCUMULATION_HEIGHT),
            random_tick_speed: rules.get(GameRules::RANDOM_TICK_SPEED),
        }
    }

    fn weather(&self) -> WeatherState {
        let weather = self.server.weather_data_ref();
        WeatherState {
            clear_weather_time: weather.get_clear_weather_time(),
            thunder_time: weather.get_thunder_time(),
            rain_time: weather.get_rain_time(),
            thundering: weather.is_thundering(),
            raining: weather.is_raining(),
        }
    }

    fn set_weather(&mut self, weather: WeatherState) {
        let data: &mut WeatherData = self.server.weather_data();
        data.set_clear_weather_time(weather.clear_weather_time);
        data.set_thunder_time(weather.thunder_time);
        data.set_rain_time(weather.rain_time);
        data.set_thundering(weather.thundering);
        data.set_raining(weather.raining);
    }

    fn wake_up_from_sleep(&mut self) {
        let dimension = self.dimension.clone();
        self.server.wake_up_from_sleep(&dimension);
    }

    fn clock_total_ticks(&self, clock: &mc_core::ident::Identifier) -> i64 {
        use crate::world::clock::ClockInstance as _;
        self.server.clock_manager().get_instance(clock).total_ticks()
    }

    fn difficulty(&self) -> Difficulty {
        self.server.world_data.lock().settings.difficulty_settings.difficulty
    }

    fn overworld_clock_time(&self) -> i64 {
        self.server.overworld_clock_time()
    }

    fn tick_scheduled_events(&mut self, game_time: i64) {
        if let Some(hook) = self.server.tick_hooks.tick_scheduled_events {
            hook(self.server, game_time);
        }
    }

    fn tick_raids(&mut self, level: &mut ServerLevel) {
        if let Some(hook) = self.server.tick_hooks.tick_raids {
            hook(self.server, level);
        }
    }

    fn broadcast_game_event_to_all(&mut self, packet: &ClientboundGameEventPacket) {
        self.server.player_list.broadcast_all(packet);
    }

    fn set_game_time(&mut self, game_time: i64) {
        self.server.world_data.lock().game_time = game_time;
    }
}
