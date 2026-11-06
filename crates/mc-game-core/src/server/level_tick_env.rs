//! `LevelTickEnv` for the server: what `ServerLevel.tick` reads from `MinecraftServer` (tick rate manager, game rules,
//! weather saved data, difficulty, clocks, player list).
//!
//! Not modelled yet (their owners have not landed): raids and the
//! environment attribute system behind `sky_light_level` / `moon_brightness` (the overworld day/night cycle is read from
//! the overworld clock with the vanilla day length instead).

use super::MinecraftServer;
use crate::server::level::ServerLevel;
use crate::server::level::level_tick::{LevelTickEnv, WeatherState};
use crate::world::entity::Entity;
use crate::world::level::DimensionKey;
use crate::world::level::gamerules::GameRules;
use crate::world::level::saveddata::weather_data::WeatherData;
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

    fn is_entity_frozen(&self, entity: &dyn Entity, count_player_passengers: i32) -> bool {
        self.server
            .tick_rate_manager
            .is_entity_frozen(entity.is_player(), count_player_passengers)
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
        self.server
            .world_data
            .lock()
            .settings
            .difficulty_settings
            .difficulty
    }

    fn overworld_clock_time(&self) -> i64 {
        self.server.overworld_clock_time()
    }

    fn tick_scheduled_events(&mut self, game_time: i64, level: &mut ServerLevel) {
        self.server.tick_scheduled_events(game_time, level);
    }

    fn broadcast_game_event_to_all(&mut self, packet: &ClientboundGameEventPacket) {
        self.server.player_list.broadcast_all(packet);
    }

    fn perform_prefixed_command(
        &mut self,
        level: &mut ServerLevel,
        source: crate::commands::CommandSourceStack,
        command: &str,
    ) {
        let commands = self.server.commands.clone();
        self.server.with_lent_level(level, |server| {
            commands.perform_prefixed_command(server, source, command)
        });
    }

    fn command_rules(&self) -> crate::server::level::level_commands::CommandRules {
        let rules = self.server.game_rules_ref();
        crate::server::level::level_commands::CommandRules {
            command_blocks_work: rules.get(GameRules::COMMAND_BLOCKS_WORK),
            command_block_output: rules.get(GameRules::COMMAND_BLOCK_OUTPUT),
            send_command_feedback: rules.get(GameRules::SEND_COMMAND_FEEDBACK),
            max_command_sequence_length: rules.get(GameRules::MAX_COMMAND_SEQUENCE_LENGTH),
        }
    }

    fn set_game_time(&mut self, game_time: i64) {
        self.server.world_data.lock().game_time = game_time;
    }

    fn teleport_through_portal(&mut self, level: &mut ServerLevel, entity: &mut dyn Entity) {
        crate::server::players::dimension_change::teleport_through_portal(
            self.server,
            level,
            entity,
        );
    }
}
