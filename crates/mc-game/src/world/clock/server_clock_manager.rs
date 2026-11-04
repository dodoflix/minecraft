//! `ServerClockManager` (`data/minecraft/world_clocks.dat`), `ServerClockInstance`, `ClockInstance`.

use indexmap::IndexMap;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_nbt::Tag;

use super::{ClockNetworkState, ClockState, ClockTimeMarker, PackedClockStates};
use crate::impl_saved_data;
use crate::util::codec::{DataResult, DecodeResult, map_decoded};
use crate::world::level::saveddata::{SavedData, SavedDataType};

/// `ClockInstance`.
pub trait ClockInstance {
    fn total_ticks(&self) -> i64;
    fn partial_tick(&self) -> f32;
    fn rate(&self) -> f32;
    fn is_paused(&self) -> bool;
}

/// What `ServerClockManager` asks of the server (`MinecraftServer`, its player list and levels).
pub trait ClockHost {
    /// `server.getGlobalGameRules().get(GameRules.ADVANCE_TIME)`.
    fn advance_time(&self) -> bool;
    /// `server.overworld().getGameTime()`.
    fn game_time(&self) -> i64;
    /// `playerList.broadcastAll(new ClientboundSetTimePacket(gameTime, updates))`.
    fn broadcast_set_time(&mut self, game_time: i64, updates: Vec<(Identifier, ClockNetworkState)>);
    /// `level.environmentAttributes().invalidateTickCache()` for every level.
    fn invalidate_environment_attributes_tick_cache(&mut self);
}

/// `ServerClockManager.MoveResult`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveResult {
    NoTimeMarkerFound,
    NotMoved,
    Moved,
}

/// `ServerClockManager.ServerClockInstance`.
#[derive(Debug, Clone)]
pub struct ServerClockInstance {
    time_markers: IndexMap<Identifier, ClockTimeMarker>,
    total_ticks: i64,
    partial_tick: f32,
    rate: f32,
    paused: bool,
}

impl Default for ServerClockInstance {
    fn default() -> Self {
        ServerClockInstance { time_markers: IndexMap::new(), total_ticks: 0, partial_tick: 0.0, rate: 1.0, paused: false }
    }
}

impl ServerClockInstance {
    pub fn load_from(&mut self, state: &ClockState) {
        self.total_ticks = state.total_ticks;
        self.partial_tick = state.partial_tick;
        self.rate = state.rate;
        self.paused = state.paused;
    }

    pub fn tick(&mut self) {
        if !self.paused {
            self.partial_tick += self.rate;
            let full_ticks = mth::floor_f32(self.partial_tick);
            self.partial_tick -= full_ticks as f32;
            self.total_ticks = self.total_ticks.wrapping_add(i64::from(full_ticks));
        }
    }

    pub fn pack_state(&self) -> ClockState {
        ClockState { total_ticks: self.total_ticks, partial_tick: self.partial_tick, rate: self.rate, paused: self.paused }
    }

    /// `packNetworkState(server)`: paused (or `advance_time` off) sends rate 0.
    pub fn pack_network_state(&self, advance_time: bool) -> ClockNetworkState {
        let paused = self.paused || !advance_time;
        ClockNetworkState { total_ticks: self.total_ticks, partial_tick: self.partial_tick, rate: if paused { 0.0 } else { self.rate } }
    }

    pub fn time_markers(&self) -> &IndexMap<Identifier, ClockTimeMarker> {
        &self.time_markers
    }
}

impl ClockInstance for ServerClockInstance {
    fn total_ticks(&self) -> i64 {
        self.total_ticks
    }
    fn partial_tick(&self) -> f32 {
        self.partial_tick
    }
    fn rate(&self) -> f32 {
        self.rate
    }
    fn is_paused(&self) -> bool {
        self.paused
    }
}

/// `ServerClockManager`.
#[derive(Debug)]
pub struct ServerClockManager {
    dirty: bool,
    packed_clock_states: PackedClockStates,
    clocks: IndexMap<Identifier, ServerClockInstance>,
}

impl_saved_data!(ServerClockManager);

impl Default for ServerClockManager {
    fn default() -> Self {
        ServerClockManager::new(PackedClockStates::EMPTY)
    }
}

impl ServerClockManager {
    fn new(packed_clock_states: PackedClockStates) -> Self {
        ServerClockManager { dirty: false, packed_clock_states, clocks: IndexMap::new() }
    }

    /// `init(server)`: one instance per registered world clock (registry order), every timeline's time markers
    /// attached to their clock, then the saved states loaded. Saved states of clocks that no longer exist are dropped
    /// (vanilla's registry-aware codec rejects such entries before this point).
    pub fn init(&mut self, world_clocks: &[Identifier], time_markers: impl IntoIterator<Item = (Identifier, ClockTimeMarker)>) {
        for clock in world_clocks {
            self.clocks.insert(clock.clone(), ServerClockInstance::default());
        }
        for (id, marker) in time_markers {
            match self.clocks.get_mut(&marker.clock) {
                Some(instance) => {
                    instance.time_markers.insert(id, marker);
                }
                None => log::warn!("Time marker {id} refers to unknown world clock {}", marker.clock),
            }
        }
        for (clock, state) in &self.packed_clock_states.clocks {
            match self.clocks.get_mut(clock) {
                Some(instance) => instance.load_from(state),
                None => log::warn!("Saved state for unknown world clock {clock}"),
            }
        }
    }

    pub fn pack_state(&self) -> PackedClockStates {
        PackedClockStates { clocks: self.clocks.iter().map(|(id, c)| (id.clone(), c.pack_state())).collect() }
    }

    /// `tick()`: `advance_time` is the global `advance_time` game rule.
    pub fn tick(&mut self, advance_time: bool) {
        if advance_time {
            for clock in self.clocks.values_mut() {
                clock.tick();
            }
            self.set_dirty();
        }
    }

    /// `getInstance`: panics for an uninitialized clock (`IllegalStateException`).
    pub fn get_instance(&self, definition: &Identifier) -> &ServerClockInstance {
        self.clocks.get(definition).unwrap_or_else(|| panic!("No clock initialized for definition: {definition}"))
    }

    fn modify_clock(&mut self, host: &mut dyn ClockHost, clock: &Identifier, action: impl FnOnce(&mut ServerClockInstance)) {
        let advance_time = host.advance_time();
        let instance = self.clocks.get_mut(clock).unwrap_or_else(|| panic!("No clock initialized for definition: {clock}"));
        action(instance);
        let updates = vec![(clock.clone(), instance.pack_network_state(advance_time))];
        host.broadcast_set_time(host.game_time(), updates);
        self.set_dirty();
        host.invalidate_environment_attributes_tick_cache();
    }

    pub fn set_total_ticks(&mut self, host: &mut dyn ClockHost, clock: &Identifier, total_ticks: i64) {
        self.modify_clock(host, clock, |i| {
            i.total_ticks = total_ticks;
            i.partial_tick = 0.0;
        });
    }

    pub fn move_to_time_marker(&mut self, host: &mut dyn ClockHost, clock: &Identifier, time_marker_id: &Identifier) -> MoveResult {
        let mut result = MoveResult::NoTimeMarkerFound;
        self.modify_clock(host, clock, |i| match i.time_markers.get(time_marker_id) {
            None => result = MoveResult::NoTimeMarkerFound,
            Some(marker) if marker.occurs_at(i.total_ticks) => result = MoveResult::NotMoved,
            Some(marker) => {
                i.total_ticks = marker.resolve_time_to_move_to(i.total_ticks);
                i.partial_tick = 0.0;
                result = MoveResult::Moved;
            }
        });
        result
    }

    pub fn add_ticks(&mut self, host: &mut dyn ClockHost, clock: &Identifier, ticks: i32) {
        self.modify_clock(host, clock, |i| i.total_ticks = i.total_ticks.wrapping_add(i64::from(ticks)).max(0));
    }

    pub fn set_paused(&mut self, host: &mut dyn ClockHost, clock: &Identifier, paused: bool) {
        self.modify_clock(host, clock, |i| i.paused = paused);
    }

    pub fn set_rate(&mut self, host: &mut dyn ClockHost, clock: &Identifier, rate: f32) {
        self.modify_clock(host, clock, |i| i.rate = rate);
    }

    /// `createFullSyncPacket` payload: `(game_time, every clock's network state)`.
    pub fn create_full_sync_state(&self, host: &dyn ClockHost) -> (i64, Vec<(Identifier, ClockNetworkState)>) {
        let advance_time = host.advance_time();
        (host.game_time(), self.clocks.iter().map(|(id, c)| (id.clone(), c.pack_network_state(advance_time))).collect())
    }

    pub fn is_at_time_marker(&self, clock: &Identifier, time_marker_id: &Identifier) -> bool {
        let instance = self.get_instance(clock);
        instance.time_markers.get(time_marker_id).is_some_and(|m| m.occurs_at(instance.total_ticks))
    }

    /// `commandTimeMarkersForClock`.
    pub fn command_time_markers_for_clock<'a>(&'a self, clock: &Identifier) -> impl Iterator<Item = &'a Identifier> {
        self.get_instance(clock).time_markers.iter().filter(|(_, m)| m.show_in_commands).map(|(id, _)| id)
    }

    /// `TYPE` (`data/minecraft/world_clocks.dat`).
    pub fn type_() -> SavedDataType<ServerClockManager> {
        fn decode(tag: &Tag) -> DecodeResult<ServerClockManager> {
            map_decoded(PackedClockStates::decode(tag), ServerClockManager::new)
        }
        fn encode(manager: &ServerClockManager) -> DataResult<Tag> {
            Ok(manager.pack_state().encode())
        }
        SavedDataType::new(Identifier::parse("minecraft:world_clocks").expect("static id"), ServerClockManager::default, decode, encode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Host {
        advance: bool,
        sent: Vec<(i64, Vec<(Identifier, ClockNetworkState)>)>,
        invalidated: u32,
    }
    impl ClockHost for Host {
        fn advance_time(&self) -> bool {
            self.advance
        }
        fn game_time(&self) -> i64 {
            77
        }
        fn broadcast_set_time(&mut self, game_time: i64, updates: Vec<(Identifier, ClockNetworkState)>) {
            self.sent.push((game_time, updates));
        }
        fn invalidate_environment_attributes_tick_cache(&mut self) {
            self.invalidated += 1;
        }
    }

    fn id(s: &str) -> Identifier {
        Identifier::parse(s).unwrap()
    }

    fn manager() -> ServerClockManager {
        let mut m = ServerClockManager::default();
        let overworld = id("minecraft:overworld");
        let markers = vec![(
            id("minecraft:noon"),
            ClockTimeMarker { clock: overworld.clone(), ticks: 6000, period_ticks: Some(24000), show_in_commands: true },
        )];
        m.init(&[overworld, id("minecraft:the_end")], markers);
        m
    }

    #[test]
    fn tick_rate_and_partial_ticks() {
        let mut m = manager();
        let ow = id("minecraft:overworld");
        let mut host = Host { advance: true, sent: vec![], invalidated: 0 };
        m.set_rate(&mut host, &ow, 1.5);
        m.tick(true);
        m.tick(true);
        let i = m.get_instance(&ow);
        assert_eq!((i.total_ticks(), i.partial_tick()), (3, 0.0));
        m.tick(false);
        assert_eq!(m.get_instance(&ow).total_ticks(), 3);
        m.set_paused(&mut host, &ow, true);
        m.tick(true);
        assert_eq!(m.get_instance(&ow).total_ticks(), 3);
        // the end clock was not paused and kept running (2 + 1 ticks at rate 1)
        assert_eq!(m.get_instance(&id("minecraft:the_end")).total_ticks(), 3);
        assert_eq!(host.invalidated, 2);
        // paused clocks and advance_time=false both send rate 0
        assert_eq!(host.sent[1].1[0].1.rate, 0.0);
        assert_eq!(host.sent[0].1[0].1.rate, 1.5);
        assert_eq!(host.sent[0].0, 77);
    }

    #[test]
    fn set_add_and_markers() {
        let mut m = manager();
        let ow = id("minecraft:overworld");
        let noon = id("minecraft:noon");
        let mut host = Host { advance: false, sent: vec![], invalidated: 0 };
        m.set_total_ticks(&mut host, &ow, 100);
        assert_eq!(host.sent[0].1[0].1.rate, 0.0, "advance_time off");
        assert_eq!(m.move_to_time_marker(&mut host, &ow, &noon), MoveResult::Moved);
        assert_eq!(m.get_instance(&ow).total_ticks(), 6000);
        assert!(m.is_at_time_marker(&ow, &noon));
        assert_eq!(m.move_to_time_marker(&mut host, &ow, &noon), MoveResult::NotMoved);
        assert_eq!(m.move_to_time_marker(&mut host, &ow, &id("minecraft:nope")), MoveResult::NoTimeMarkerFound);
        m.add_ticks(&mut host, &ow, -7000);
        assert_eq!(m.get_instance(&ow).total_ticks(), 0);
        assert_eq!(m.command_time_markers_for_clock(&ow).collect::<Vec<_>>(), [&noon]);
        assert_eq!(m.command_time_markers_for_clock(&id("minecraft:the_end")).count(), 0);
    }

    #[test]
    fn saved_state_round_trip_and_init() {
        let ty = ServerClockManager::type_();
        let tag = mc_nbt::snbt::parse("{\"minecraft:overworld\":{total_ticks:1199L,rate:2.0f,paused:1b,partial_tick:0.5f},\"minecraft:the_end\":{total_ticks:5L}}").unwrap();
        let mut m = (ty.decode)(&tag).unwrap();
        m.init(&[id("minecraft:overworld"), id("minecraft:the_end")], vec![]);
        let i = m.get_instance(&id("minecraft:overworld"));
        assert_eq!((i.total_ticks(), i.partial_tick(), i.rate(), i.is_paused()), (1199, 0.5, 2.0, true));
        assert_eq!((ty.encode)(&m).unwrap(), tag);
        assert!((ty.decode)(&mc_nbt::snbt::parse("{\"minecraft:overworld\":{total_ticks:1L,rate:0.0f}}").unwrap()).is_err_and(|p| p.value.is_some()));
    }
}
