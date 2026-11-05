//! `LevelTickEnv` for the server: what `ServerLevel.tick` reads from `MinecraftServer` (tick rate manager, game rules,
//! weather saved data, difficulty, clocks, player list).
//!
//! Not modelled yet (their owners have not landed): raids, the scheduled-events timer queue (needs the function manager) and the
//! environment attribute system behind `sky_light_level` / `moon_brightness` (the overworld day/night cycle is read from
//! the overworld clock with the vanilla day length instead).

use super::MinecraftServer;
use crate::server::level::ServerLevel;
use crate::server::level::level_tick::{LevelTickEnv, WeatherState};
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

    fn teleport_through_portal(&mut self, level: &mut ServerLevel, entity: &mut dyn Entity) {
        use crate::server::players::dimension_change::teleport_same_dimension;
        let Some(process) = entity.base().portal_process.clone() else { return };
        let Some(transition) = process.get_portal_destination(self.server, level, entity) else { return };
        if transition.level == self.dimension {
            if !transition.as_passenger {
                level.stop_riding(entity);
            }
            teleport_same_dimension(level, entity, &transition);
        } else {
            // cross-dimension hand-over of an entity that is out of its level while the level ticks is not wired:
            // `dimension_change::teleport_entity/teleport_player` need the entity in storage (see STATUS gaps)
            log::warn!("portal transition to {} of entity {} is not executed during the level tick", transition.level, entity.id());
        }
    }
}
