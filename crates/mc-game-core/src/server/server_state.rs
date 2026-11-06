//! `MinecraftServer`'s accessors for the saved data it owns (game rules, weather, clocks), the host adapters of the
//! tick-rate and clock managers, and the game-rule change listener (`onGameRuleChanged`).

use super::MinecraftServer;
use crate::registry::keys;
use crate::server::notifications::NotificationService;
use crate::server::tick_rate_manager::{ServerTickRateManager, TickRateHost};
use crate::world::clock::{
    ClockHost, ClockInstance, ClockNetworkState, ServerClockManager,
    time_markers_from_timeline_json, world_clocks,
};
use crate::world::level::DimensionKey;
use crate::world::level::gamerules::{AnyGameRule, GameRuleListener, GameRuleValue, GameRules};
use crate::world::level::saveddata::SavedDataType;
use crate::world::level::saveddata::weather_data::WeatherData;
use mc_core::ident::Identifier;
use mc_protocol::packets::play as packets;
use serde_json::Value;

/// `MinecraftServer` fields of typed saved data: the types are kept to look the instances up again.
pub struct SavedDataTypes {
    pub weather: SavedDataType<WeatherData>,
    pub clocks: SavedDataType<ServerClockManager>,
    pub scoreboard: SavedDataType<crate::world::scores::ScoreboardSaveData>,
    pub custom_boss_events: SavedDataType<super::bossevents::CustomBossEvents>,
    pub random_sequences:
        SavedDataType<crate::world::level::saveddata::random_sequences::RandomSequences>,
    pub stopwatches: SavedDataType<crate::world::level::saveddata::stopwatches::Stopwatches>,
    pub scheduled_events: SavedDataType<crate::util::timer_queue::TimerQueue<MinecraftServer>>,
    pub wandering_trader:
        SavedDataType<crate::world::level::saveddata::wandering_trader_data::WanderingTraderData>,
}

impl SavedDataTypes {
    pub fn new() -> Self {
        Self {
            weather: WeatherData::type_(),
            clocks: ServerClockManager::type_(),
            scoreboard: crate::world::scores::ScoreboardSaveData::type_(),
            custom_boss_events: super::bossevents::CustomBossEvents::type_(),
            random_sequences:
                crate::world::level::saveddata::random_sequences::RandomSequences::type_(),
            stopwatches: crate::world::level::saveddata::stopwatches::Stopwatches::type_(),
            scheduled_events: crate::util::timer_queue::TimerQueue::<MinecraftServer>::type_(),
            wandering_trader:
                crate::world::level::saveddata::wandering_trader_data::WanderingTraderData::type_(),
        }
    }
}

impl Default for SavedDataTypes {
    fn default() -> Self {
        Self::new()
    }
}

/// `ClockHost` over the parts of the server that are not the saved data holding the clock manager.
struct ClockHostAdapter<'a> {
    advance_time: bool,
    game_time: i64,
    player_list: &'a crate::server::players::PlayerList,
    world_clock_ids: &'a crate::registry::Registry,
}

impl ClockHost for ClockHostAdapter<'_> {
    fn advance_time(&self) -> bool {
        self.advance_time
    }
    fn game_time(&self) -> i64 {
        self.game_time
    }
    fn broadcast_set_time(
        &mut self,
        game_time: i64,
        updates: Vec<(Identifier, ClockNetworkState)>,
    ) {
        self.player_list
            .broadcast_all(&packets::ClientboundSetTimePacket {
                game_time,
                clock_updates: to_clock_updates(self.world_clock_ids, updates),
            });
    }
    fn invalidate_environment_attributes_tick_cache(&mut self) {}
}

fn to_clock_updates(
    registry: &crate::registry::Registry,
    updates: Vec<(Identifier, ClockNetworkState)>,
) -> Vec<(i32, packets::ClockNetworkState)> {
    updates
        .into_iter()
        .filter_map(|(clock, state)| {
            let id = registry.id_of(&clock)?;
            Some((
                id as i32,
                packets::ClockNetworkState {
                    total_ticks: state.total_ticks,
                    partial_tick: state.partial_tick,
                    rate: state.rate,
                },
            ))
        })
        .collect()
}

impl MinecraftServer {
    /// `getGameRules()` for `&self` readers (the rules were loaded in the constructor).
    pub fn game_rules_ref(&self) -> &GameRules {
        self.saved_data
            .peek(&self.game_rules_type)
            .expect("game rules are loaded by the constructor")
    }

    /// `getWeatherData()`.
    pub fn weather_data(&mut self) -> &mut WeatherData {
        self.saved_data.compute_if_absent(&self.saved_types.weather)
    }

    pub fn weather_data_ref(&self) -> &WeatherData {
        self.saved_data
            .peek(&self.saved_types.weather)
            .expect("weather data is loaded by the constructor")
    }

    /// `setWeatherParameters`.
    pub fn set_weather_parameters(
        &mut self,
        clear_time: i32,
        rain_time: i32,
        raining: bool,
        thundering: bool,
    ) {
        let weather = self.weather_data();
        weather.set_clear_weather_time(clear_time);
        weather.set_rain_time(rain_time);
        weather.set_thunder_time(rain_time);
        weather.set_raining(raining);
        weather.set_thundering(thundering);
    }

    /// `clockManager()`.
    pub fn clock_manager(&self) -> &ServerClockManager {
        self.saved_data
            .peek(&self.saved_types.clocks)
            .expect("the clock manager is loaded by the constructor")
    }

    /// `init(server)` of the clock manager: one clock per `world_clock` registry entry, the time markers of every
    /// `timeline` attached to their clock.
    pub(super) fn init_clock_manager(&mut self) {
        let access = self.registries.composite_access();
        let clocks: Vec<Identifier> = access
            .registry(keys::WORLD_CLOCK)
            .map(|r| r.names())
            .unwrap_or_default();
        let mut markers = Vec::new();
        if let Some(timelines) = access.registry(keys::TIMELINE) {
            for entry in timelines.entries() {
                match time_markers_from_timeline_json(&entry.json) {
                    Ok(found) => markers.extend(found),
                    Err(e) => log::warn!(
                        "Failed to read the time markers of timeline {}: {e}",
                        entry.id
                    ),
                }
            }
        }
        self.saved_data
            .compute_if_absent(&self.saved_types.clocks)
            .init(&clocks, markers);
    }

    /// `overworld().getOverworldClockTime()`: the total ticks of the `minecraft:overworld` world clock.
    pub fn overworld_clock_time(&self) -> i64 {
        let clock = Identifier::parse(world_clocks::OVERWORLD).expect("valid id");
        self.clock_manager().get_instance(&clock).total_ticks()
    }

    /// `clockManager.tick()` (only while the tick rate manager runs game elements).
    pub(super) fn tick_clocks(&mut self) {
        let advance_time = self.game_rules_ref().get(GameRules::ADVANCE_TIME);
        self.saved_data
            .compute_if_absent(&self.saved_types.clocks)
            .tick(advance_time);
    }

    /// `createFullSyncPacket`.
    pub fn create_clock_full_sync_packet(&self) -> packets::ClientboundSetTimePacket {
        let host = ClockHostAdapter {
            advance_time: self.game_rules_ref().get(GameRules::ADVANCE_TIME),
            game_time: self.world_data.lock().game_time,
            player_list: &self.player_list,
            world_clock_ids: self
                .registries
                .composite_access()
                .registry_or_throw(keys::WORLD_CLOCK),
        };
        let (game_time, updates) = self.clock_manager().create_full_sync_state(&host);
        packets::ClientboundSetTimePacket {
            game_time,
            clock_updates: to_clock_updates(host.world_clock_ids, updates),
        }
    }

    /// Run `f` with the clock manager and a [`ClockHost`] over the rest of the server (clock changes broadcast
    /// `ClientboundSetTimePacket`s).
    pub fn with_clock_manager<R>(
        &mut self,
        f: impl FnOnce(&mut ServerClockManager, &mut dyn ClockHost) -> R,
    ) -> R {
        let advance_time = self
            .saved_data
            .peek(&self.game_rules_type)
            .map_or(true, |rules| rules.get(GameRules::ADVANCE_TIME));
        let mut host = ClockHostAdapter {
            advance_time,
            game_time: self.world_data.lock().game_time,
            player_list: &self.player_list,
            world_clock_ids: self
                .registries
                .composite_access()
                .registry_or_throw(keys::WORLD_CLOCK),
        };
        f(
            self.saved_data.compute_if_absent(&self.saved_types.clocks),
            &mut host,
        )
    }

    /// `server.clockManager().moveToTimeMarker(defaultClock, WAKE_UP_FROM_SLEEP)` for `dimension`'s default clock.
    pub(super) fn wake_up_from_sleep(&mut self, dimension: &DimensionKey) {
        let Some(clock) = self.default_clock_of(dimension) else {
            return;
        };
        let marker = Identifier::parse("minecraft:wake_up_from_sleep").expect("valid id");
        self.with_clock_manager(|clocks, host| {
            clocks.move_to_time_marker(host, &clock, &marker);
        });
    }

    /// `DimensionType.defaultClock()` of `dimension`.
    pub(crate) fn default_clock_of(&self, dimension: &DimensionKey) -> Option<Identifier> {
        let stem = self.registries.composite_access().level_stem(dimension)?;
        stem.dimension_type.value().default_clock.clone()
    }

    // ------------------------------------------------------------------ tick rate manager host

    /// Run `f` with the tick rate manager and a [`TickRateHost`] over the rest of the server (vanilla's manager holds
    /// the server; here it is taken out while it runs).
    pub fn with_tick_rate_manager<R>(
        &mut self,
        f: impl FnOnce(&mut ServerTickRateManager, &mut dyn TickRateHost) -> R,
    ) -> R {
        let mut manager = std::mem::take(&mut self.tick_rate_manager);
        let result = f(&mut manager, self);
        self.tick_rate_manager = manager;
        result
    }

    pub(super) fn check_should_sprint_this_tick(&mut self) -> bool {
        self.with_tick_rate_manager(|manager, host| manager.check_should_sprint_this_tick(host))
    }

    // ------------------------------------------------------------------ onGameRuleChanged

    /// `onGameRuleChanged(rule, value)`.
    pub fn on_game_rule_changed(&mut self, rule: AnyGameRule, value: GameRuleValue) {
        self.notification_manager.on_game_rule_changed(
            &rule.get_identifier().to_string(),
            &notification_value(value),
        );
        let enabled = Self::game_rule_value_bool(value);
        if rule == GameRules::REDUCED_DEBUG_INFO.erase() {
            let event: i8 = if enabled { 22 } else { 23 };
            for player in self.player_list.get_players() {
                player
                    .connection
                    .send(&packets::ClientboundEntityEventPacket {
                        entity_id: player.id,
                        event_id: event,
                    });
            }
        } else if rule == GameRules::LIMITED_CRAFTING.erase()
            || rule == GameRules::IMMEDIATE_RESPAWN.erase()
        {
            let event = if rule == GameRules::LIMITED_CRAFTING.erase() {
                packets::ClientboundGameEventPacket::LIMITED_CRAFTING
            } else {
                packets::ClientboundGameEventPacket::IMMEDIATE_RESPAWN
            };
            self.player_list
                .broadcast_all(&packets::ClientboundGameEventPacket {
                    event,
                    param: if enabled { 1.0 } else { 0.0 },
                });
        } else if rule == GameRules::SPAWN_MONSTERS.erase() {
            self.update_mob_spawning_flags();
        } else if rule == GameRules::ADVANCE_TIME.erase() {
            let packet = self.create_clock_full_sync_packet();
            self.player_list.broadcast_all(&packet);
        } else if rule == GameRules::LOCATOR_BAR.erase() {
            for level in self.all_levels_mut() {
                level.waypoint_locator_bar_changed(enabled);
            }
        }
    }
}

impl GameRuleListener for MinecraftServer {
    fn on_game_rule_changed(&mut self, rule: AnyGameRule, value: GameRuleValue) {
        MinecraftServer::on_game_rule_changed(self, rule, value);
    }
}

impl TickRateHost for MinecraftServer {
    fn broadcast_ticking_state(&mut self, tick_rate: f32, is_frozen: bool) {
        self.player_list
            .broadcast_all(&packets::ClientboundTickingStatePacket {
                tick_rate,
                is_frozen,
            });
    }

    fn broadcast_ticking_step(&mut self, frozen_ticks_to_run: i32) {
        self.player_list
            .broadcast_all(&packets::ClientboundTickingStepPacket {
                tick_steps: frozen_ticks_to_run,
            });
    }

    fn on_tick_rate_changed(&mut self, tickrate: f32, sprinting: bool) {
        MinecraftServer::on_tick_rate_changed(self, tickrate, sprinting);
    }

    fn send_sprint_report(&mut self, ticks_per_second: i32, milliseconds_per_tick: String) {
        // `createCommandSourceStack().sendSuccess(.., true)`: the server logs the message (`sendSystemMessage`), then
        // `broadcastToAdmins` wraps it in `chat.type.admin` for the operators.
        let message = mc_text::Component::translatable(
            "commands.tick.sprint.report",
            vec![ticks_per_second.into(), milliseconds_per_tick.into()],
        );
        log::info!("System chat: {}", message.get_string());
        let broadcast = mc_text::Component::translatable(
            "chat.type.admin",
            vec![mc_text::Component::literal("Server").into(), message.into()],
        )
        .with_formats(&[
            mc_text::ChatFormatting::Gray,
            mc_text::ChatFormatting::Italic,
        ]);
        let overworld = crate::server::world_dimensions::overworld_key();
        crate::commands::host::broadcast_to_admins(
            self,
            crate::commands::CommandSourceKind::Server,
            &overworld,
            &broadcast,
        );
    }
}

fn notification_value(value: GameRuleValue) -> crate::server::notifications::GameRuleValue {
    match value {
        GameRuleValue::Bool(b) => crate::server::notifications::GameRuleValue::Bool(b),
        GameRuleValue::Int(i) => crate::server::notifications::GameRuleValue::Int(i),
    }
}
