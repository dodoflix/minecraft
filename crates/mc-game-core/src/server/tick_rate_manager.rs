//! `TickRateManager` + `ServerTickRateManager` (`/tick`): tick rate, freeze, step, sprint.
//!
//! Game-thread plain data. Everything vanilla does through `MinecraftServer`/`PlayerList` goes through
//! [`TickRateHost`]; methods that reach the host take it as an argument so the manager can live as a field of the
//! server (use the take-out pattern: `mem::take` the manager, call, put it back).

use mc_core::math::java::max_f32;

use crate::util::time_util::{
    MILLISECONDS_PER_SECOND, NANOSECONDS_PER_MILLISECOND, NANOSECONDS_PER_SECOND,
};
use crate::util::util::get_nanos;

/// `TickRateManager.MIN_TICKRATE`.
pub const MIN_TICKRATE: f32 = 1.0;

/// What `ServerTickRateManager` needs from the server and its player list.
pub trait TickRateHost {
    /// `playerList.broadcastAll(ClientboundTickingStatePacket.from(this))`.
    fn broadcast_ticking_state(&mut self, tick_rate: f32, is_frozen: bool);
    /// `playerList.broadcastAll(ClientboundTickingStepPacket.from(this))`.
    fn broadcast_ticking_step(&mut self, frozen_ticks_to_run: i32);
    /// `server.onTickRateChanged()`; the manager is taken out of the server while it runs, so its (already updated)
    /// tick rate and sprint state come as arguments.
    fn on_tick_rate_changed(&mut self, tickrate: f32, sprinting: bool);
    /// `server.createCommandSourceStack().sendSuccess(() -> translatable("commands.tick.sprint.report", ticksPerSecond,
    /// millisecondsPerTick), true)`.
    fn send_sprint_report(&mut self, ticks_per_second: i32, milliseconds_per_tick: String);
}

/// `ClientboundTickingStatePacket.from(manager)` payload.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickingState {
    pub tick_rate: f32,
    pub is_frozen: bool,
}

/// `ClientboundTickingStepPacket.from(manager)` payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickingStep {
    pub tick_steps: i32,
}

#[derive(Debug, Clone)]
pub struct ServerTickRateManager {
    // TickRateManager
    tickrate: f32,
    nanoseconds_per_tick: i64,
    frozen_ticks_to_run: i32,
    run_game_elements: bool,
    is_frozen: bool,
    // ServerTickRateManager
    remaining_sprint_ticks: i64,
    sprint_tick_start_time: i64,
    sprint_time_spend: i64,
    scheduled_current_sprint_ticks: i64,
    previous_is_frozen: bool,
}

impl Default for ServerTickRateManager {
    fn default() -> Self {
        ServerTickRateManager {
            tickrate: 20.0,
            nanoseconds_per_tick: NANOSECONDS_PER_SECOND / 20,
            frozen_ticks_to_run: 0,
            run_game_elements: true,
            is_frozen: false,
            remaining_sprint_ticks: 0,
            sprint_tick_start_time: 0,
            sprint_time_spend: 0,
            scheduled_current_sprint_ticks: 0,
            previous_is_frozen: false,
        }
    }
}

impl ServerTickRateManager {
    pub fn new() -> Self {
        Self::default()
    }

    // --- TickRateManager ---

    pub fn tickrate(&self) -> f32 {
        self.tickrate
    }

    pub fn milliseconds_per_tick(&self) -> f32 {
        self.nanoseconds_per_tick as f32 / NANOSECONDS_PER_MILLISECOND as f32
    }

    pub fn nanoseconds_per_tick(&self) -> i64 {
        self.nanoseconds_per_tick
    }

    pub fn runs_normally(&self) -> bool {
        self.run_game_elements
    }

    pub fn is_stepping_forward(&self) -> bool {
        self.frozen_ticks_to_run > 0
    }

    pub fn set_frozen_ticks_to_run(&mut self, timeout: i32) {
        self.frozen_ticks_to_run = timeout;
    }

    pub fn frozen_ticks_to_run(&self) -> i32 {
        self.frozen_ticks_to_run
    }

    pub fn is_frozen(&self) -> bool {
        self.is_frozen
    }

    /// `tick()`.
    pub fn tick(&mut self) {
        self.run_game_elements = !self.is_frozen || self.frozen_ticks_to_run > 0;
        if self.frozen_ticks_to_run > 0 {
            self.frozen_ticks_to_run -= 1;
        }
    }

    /// `isEntityFrozen(entity)` with the entity facts it reads: players and entities carrying players never freeze.
    pub fn is_entity_frozen(&self, entity_is_player: bool, count_player_passengers: i32) -> bool {
        !self.runs_normally() && !entity_is_player && count_player_passengers <= 0
    }

    // --- ServerTickRateManager ---

    /// `ClientboundTickingStatePacket.from`.
    pub fn ticking_state(&self) -> TickingState {
        TickingState {
            tick_rate: self.tickrate,
            is_frozen: self.is_frozen,
        }
    }

    /// `ClientboundTickingStepPacket.from`.
    pub fn ticking_step(&self) -> TickingStep {
        TickingStep {
            tick_steps: self.frozen_ticks_to_run,
        }
    }

    /// `updateJoiningPlayer`: the two packets to send to the joining player, in order.
    pub fn update_joining_player(&self) -> (TickingState, TickingStep) {
        (self.ticking_state(), self.ticking_step())
    }

    pub fn is_sprinting(&self) -> bool {
        self.scheduled_current_sprint_ticks > 0
    }

    fn update_state_to_clients(&self, host: &mut dyn TickRateHost) {
        host.broadcast_ticking_state(self.tickrate, self.is_frozen);
    }

    fn update_step_ticks(&self, host: &mut dyn TickRateHost) {
        host.broadcast_ticking_step(self.frozen_ticks_to_run);
    }

    pub fn set_frozen(&mut self, frozen: bool, host: &mut dyn TickRateHost) {
        self.is_frozen = frozen;
        self.update_state_to_clients(host);
    }

    /// `setTickRate(rate)`.
    pub fn set_tick_rate(&mut self, rate: f32, host: &mut dyn TickRateHost) {
        self.tickrate = max_f32(rate, MIN_TICKRATE);
        self.nanoseconds_per_tick =
            (NANOSECONDS_PER_SECOND as f64 / f64::from(self.tickrate)) as i64;
        host.on_tick_rate_changed(self.tickrate, self.is_sprinting());
        self.update_state_to_clients(host);
    }

    /// `stepGameIfPaused`.
    pub fn step_game_if_paused(&mut self, ticks: i32, host: &mut dyn TickRateHost) -> bool {
        if !self.is_frozen {
            return false;
        }
        self.frozen_ticks_to_run = ticks;
        self.update_step_ticks(host);
        true
    }

    pub fn stop_stepping(&mut self, host: &mut dyn TickRateHost) -> bool {
        if self.frozen_ticks_to_run > 0 {
            self.frozen_ticks_to_run = 0;
            self.update_step_ticks(host);
            true
        } else {
            false
        }
    }

    pub fn stop_sprinting(&mut self, host: &mut dyn TickRateHost) -> bool {
        if self.remaining_sprint_ticks > 0 {
            self.finish_tick_sprint(host);
            true
        } else {
            false
        }
    }

    /// `requestGameToSprint`: returns whether a running sprint was interrupted.
    pub fn request_game_to_sprint(&mut self, time: i32, host: &mut dyn TickRateHost) -> bool {
        let interrupted = self.remaining_sprint_ticks > 0;
        self.sprint_time_spend = 0;
        self.scheduled_current_sprint_ticks = i64::from(time);
        self.remaining_sprint_ticks = i64::from(time);
        self.previous_is_frozen = self.is_frozen;
        self.set_frozen(false, host);
        interrupted
    }

    fn finish_tick_sprint(&mut self, host: &mut dyn TickRateHost) {
        let completed_ticks = self.scheduled_current_sprint_ticks - self.remaining_sprint_ticks;
        let milliseconds_to_complete =
            (self.sprint_time_spend as f64).max(1.0) / NANOSECONDS_PER_MILLISECOND as f64;
        let ticks_per_second = (MILLISECONDS_PER_SECOND.wrapping_mul(completed_ticks) as f64
            / milliseconds_to_complete) as i32;
        let milliseconds_per_tick = if completed_ticks == 0 {
            crate::util::java_format::format_fixed(f64::from(self.milliseconds_per_tick()), 2)
        } else {
            crate::util::java_format::format_fixed(
                milliseconds_to_complete / completed_ticks as f64,
                2,
            )
        };
        self.scheduled_current_sprint_ticks = 0;
        self.sprint_time_spend = 0;
        host.send_sprint_report(ticks_per_second, milliseconds_per_tick);
        self.remaining_sprint_ticks = 0;
        self.set_frozen(self.previous_is_frozen, host);
        host.on_tick_rate_changed(self.tickrate, self.is_sprinting());
    }

    /// `checkShouldSprintThisTick`.
    pub fn check_should_sprint_this_tick(&mut self, host: &mut dyn TickRateHost) -> bool {
        if !self.run_game_elements {
            false
        } else if self.remaining_sprint_ticks > 0 {
            self.sprint_tick_start_time = get_nanos();
            self.remaining_sprint_ticks -= 1;
            true
        } else {
            self.finish_tick_sprint(host);
            false
        }
    }

    pub fn end_tick_work(&mut self) {
        self.sprint_time_spend += get_nanos() - self.sprint_tick_start_time;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Host {
        events: Vec<String>,
    }
    impl TickRateHost for Host {
        fn broadcast_ticking_state(&mut self, tick_rate: f32, is_frozen: bool) {
            self.events.push(format!("state {tick_rate} {is_frozen}"));
        }
        fn broadcast_ticking_step(&mut self, frozen_ticks_to_run: i32) {
            self.events.push(format!("step {frozen_ticks_to_run}"));
        }
        fn on_tick_rate_changed(&mut self, _tickrate: f32, _sprinting: bool) {
            self.events.push("changed".into());
        }
        fn send_sprint_report(&mut self, tps: i32, mspt: String) {
            self.events.push(format!("report {tps} {mspt}"));
        }
    }

    #[test]
    fn tick_rate_clamps_and_derives_nanos() {
        let mut m = ServerTickRateManager::new();
        let mut h = Host::default();
        assert_eq!(
            (
                m.tickrate(),
                m.nanoseconds_per_tick(),
                m.milliseconds_per_tick()
            ),
            (20.0, 50_000_000, 50.0)
        );
        m.set_tick_rate(0.1, &mut h);
        assert_eq!(
            (m.tickrate(), m.nanoseconds_per_tick()),
            (1.0, 1_000_000_000)
        );
        m.set_tick_rate(100.0, &mut h);
        assert_eq!(
            (m.milliseconds_per_tick(), m.nanoseconds_per_tick()),
            (10.0, 10_000_000)
        );
        m.set_tick_rate(3.0, &mut h);
        assert_eq!(m.nanoseconds_per_tick(), 333_333_333);
        assert_eq!(h.events[0], "changed");
        assert_eq!(h.events[1], "state 1 false");
    }

    #[test]
    fn freeze_and_step() {
        let mut m = ServerTickRateManager::new();
        let mut h = Host::default();
        assert!(!m.step_game_if_paused(3, &mut h), "not frozen: no step");
        m.set_frozen(true, &mut h);
        assert!(m.step_game_if_paused(2, &mut h));
        let mut ran = vec![];
        for _ in 0..4 {
            m.tick();
            ran.push(m.runs_normally());
        }
        assert_eq!(ran, [true, true, false, false]);
        assert!(m.is_entity_frozen(false, 0));
        assert!(!m.is_entity_frozen(true, 0));
        assert!(!m.is_entity_frozen(false, 1));
        assert!(!m.stop_stepping(&mut h));
        m.set_frozen_ticks_to_run(5);
        assert!(m.stop_stepping(&mut h));
        assert_eq!(h.events.last().unwrap(), "step 0");
    }

    #[test]
    fn sprint_lifecycle() {
        let mut m = ServerTickRateManager::new();
        let mut h = Host::default();
        m.set_frozen(true, &mut h);
        assert!(!m.request_game_to_sprint(2, &mut h));
        assert!(m.is_sprinting());
        assert!(!m.is_frozen(), "sprint unfreezes");
        m.tick();
        assert!(m.check_should_sprint_this_tick(&mut h));
        m.end_tick_work();
        m.tick();
        assert!(m.check_should_sprint_this_tick(&mut h));
        m.end_tick_work();
        m.tick();
        assert!(!m.check_should_sprint_this_tick(&mut h), "finished");
        assert!(!m.is_sprinting());
        assert!(m.is_frozen(), "freeze state restored");
        let report = h.events.iter().find(|e| e.starts_with("report")).unwrap();
        assert!(report.split(' ').nth(2).unwrap().contains('.'), "{report}");
        assert_eq!(h.events.last().unwrap(), "changed");
        assert!(!m.stop_sprinting(&mut h));
    }

    #[test]
    fn stop_sprinting_reports_zero_ticks() {
        let mut m = ServerTickRateManager::new();
        let mut h = Host::default();
        m.request_game_to_sprint(10, &mut h);
        assert!(m.stop_sprinting(&mut h));
        // no completed ticks: ticks/s 0, ms/tick falls back to the tick rate's 50.00
        assert!(
            h.events.contains(&"report 0 50.00".to_string()),
            "{:?}",
            h.events
        );
    }
}
