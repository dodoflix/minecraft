//! The arithmetic of `MinecraftServer`'s 20 TPS loop, kept free of server state so it can be tested exactly:
//! overload detection (`runServer`), the 100-tick tick-time ring + smoothing (`tickServer`) and the autosave interval.

use super::shared::get_nanos;

pub const NANOSECONDS_PER_SECOND: i64 = 1_000_000_000;
pub const NANOSECONDS_PER_MILLISECOND: i64 = 1_000_000;
/// `MinecraftServer.OVERLOADED_THRESHOLD_NANOS`.
const OVERLOADED_THRESHOLD_NANOS: i64 = 20 * NANOSECONDS_PER_SECOND / 20;
/// `MinecraftServer.OVERLOADED_WARNING_INTERVAL_NANOS`.
const OVERLOADED_WARNING_INTERVAL_NANOS: i64 = 10 * NANOSECONDS_PER_SECOND;
/// `MinecraftServer.TICK_STATS_SPAN`.
pub const TICK_STATS_SPAN: usize = 100;
/// `MinecraftServer.AVERAGE_TICK_TIME_SMOOTHING`.
const AVERAGE_TICK_TIME_SMOOTHING: f32 = 0.8;
/// `MinecraftServer.MIMINUM_AUTOSAVE_TICKS`.
const MINIMUM_AUTOSAVE_TICKS: i32 = 100;
/// `MinecraftServer.AUTOSAVE_INTERVAL` (ticks until the first autosave).
pub const AUTOSAVE_INTERVAL: i32 = 6000;
/// `MinecraftServer.PREPARE_LEVELS_DEFAULT_DELAY_NANOS`.
pub const PREPARE_LEVELS_DEFAULT_DELAY_NANOS: i64 = 10 * NANOSECONDS_PER_MILLISECOND;
/// `MinecraftServer.STATUS_EXPIRE_TIME_NANOS`.
pub const STATUS_EXPIRE_TIME_NANOS: i64 = 5 * NANOSECONDS_PER_SECOND;

/// A "Can't keep up!" warning: the server is `millis` ms or `ticks` ticks behind.
#[derive(Debug, PartialEq, Eq)]
pub struct Overload {
    pub millis: i64,
    pub ticks: i64,
}

/// The overload check of `runServer` for a non-sprinting tick. On a warning, `*next_tick_time_nanos` is advanced by
/// the missed ticks and `*last_overload_warning_nanos` reset, exactly like vanilla.
pub fn check_overloaded(
    now_nanos: i64,
    next_tick_time_nanos: &mut i64,
    last_overload_warning_nanos: &mut i64,
    this_tick_nanos: i64,
) -> Option<Overload> {
    let behind_time_nanos = now_nanos - *next_tick_time_nanos;
    if behind_time_nanos > OVERLOADED_THRESHOLD_NANOS + 20 * this_tick_nanos
        && *next_tick_time_nanos - *last_overload_warning_nanos
            >= OVERLOADED_WARNING_INTERVAL_NANOS + 100 * this_tick_nanos
    {
        let ticks = behind_time_nanos / this_tick_nanos;
        *next_tick_time_nanos += ticks * this_tick_nanos;
        *last_overload_warning_nanos = *next_tick_time_nanos;
        Some(Overload {
            millis: behind_time_nanos / NANOSECONDS_PER_MILLISECOND,
            ticks,
        })
    } else {
        None
    }
}

/// `tickTimesNanos`/`aggregatedTickTimesNanos`/`smoothedTickTimeMillis`.
pub struct TickTimes {
    tick_times_nanos: [i64; TICK_STATS_SPAN],
    aggregated_tick_times_nanos: i64,
    smoothed_tick_time_millis: f32,
}

impl Default for TickTimes {
    fn default() -> Self {
        Self {
            tick_times_nanos: [0; TICK_STATS_SPAN],
            aggregated_tick_times_nanos: 0,
            smoothed_tick_time_millis: 0.0,
        }
    }
}

impl TickTimes {
    /// The "tallying" step of `tickServer` for the tick numbered `tick_count` (already incremented).
    pub fn record(&mut self, tick_count: i32, tick_time_nanos: i64) {
        let tick_index = tick_count.rem_euclid(TICK_STATS_SPAN as i32) as usize;
        self.aggregated_tick_times_nanos =
            self.aggregated_tick_times_nanos - self.tick_times_nanos[tick_index] + tick_time_nanos;
        self.tick_times_nanos[tick_index] = tick_time_nanos;
        self.smoothed_tick_time_millis = self.smoothed_tick_time_millis
            * AVERAGE_TICK_TIME_SMOOTHING
            + tick_time_nanos as f32 / NANOSECONDS_PER_MILLISECOND as f32 * 0.19999999;
    }

    /// `getCurrentSmoothedTickTime`.
    pub fn smoothed_tick_time_millis(&self) -> f32 {
        self.smoothed_tick_time_millis
    }

    /// `getAverageTickTimeNanos`.
    pub fn average_tick_time_nanos(&self, tick_count: i32) -> i64 {
        self.aggregated_tick_times_nanos / i64::from(tick_count.max(1).min(TICK_STATS_SPAN as i32))
    }

    /// `getTickTimesNanos`.
    pub fn tick_times_nanos(&self) -> &[i64; TICK_STATS_SPAN] {
        &self.tick_times_nanos
    }
}

/// `computeNextAutosaveInterval` given the current ticks per second (`tickrate()`, or while sprinting
/// `1e9 / (averageTickTime + 1)`).
pub fn compute_next_autosave_interval(ticks_per_second: f32) -> i32 {
    MINIMUM_AUTOSAVE_TICKS.max((ticks_per_second * 300.0) as i32)
}

/// `1e9f / (float)(averageTickTimeNanos + 1)`: the sprint-time tick rate estimate for [`compute_next_autosave_interval`].
pub fn sprint_ticks_per_second(average_tick_time_nanos: i64) -> f32 {
    NANOSECONDS_PER_SECOND as f32 / (average_tick_time_nanos + 1) as f32
}

/// `Util.getNanos() + delay`.
pub fn now_plus(delay_nanos: i64) -> i64 {
    get_nanos() + delay_nanos
}

#[cfg(test)]
mod tests {
    use super::*;

    const TICK: i64 = 50_000_000;

    #[test]
    fn overload_warning_and_catch_up() {
        let mut next = 0;
        let mut last = 0;
        // just under the threshold (1000 ms + 20 ticks): nothing
        assert_eq!(
            check_overloaded(2_000_000_000, &mut next, &mut last, TICK),
            None
        );
        // the warning interval since the last one (10 s + 100 ticks) has not elapsed
        let mut next = 14_000_000_000;
        let mut last = 0;
        assert_eq!(
            check_overloaded(next + 2_000_000_001, &mut next, &mut last, TICK),
            None
        );
        let mut next = 15_000_000_000;
        let mut last = 0;
        let warning = check_overloaded(15_000_000_000 + 3_236_000_000, &mut next, &mut last, TICK);
        assert_eq!(
            warning,
            Some(Overload {
                millis: 3236,
                ticks: 64
            })
        );
        assert_eq!(next, 15_000_000_000 + 64 * TICK);
        assert_eq!(last, next);
    }

    #[test]
    fn tick_ring_and_smoothing() {
        let mut t = TickTimes::default();
        for tick in 1..=250 {
            t.record(tick, 1_000_000 * i64::from(tick % 7));
        }
        let expected: i64 = (151..=250)
            .map(|tick| 1_000_000 * i64::from(tick % 7))
            .sum();
        assert_eq!(t.average_tick_time_nanos(250), expected / 100);
        assert_eq!(t.average_tick_time_nanos(0), expected); // divisor is max(1, min(100, tickCount))
        assert_eq!(t.tick_times_nanos()[0], 1_000_000 * (200 % 7));
        // f32 recurrence: 0.8f * prev + ms * 0.19999999f
        let mut s = 0.0f32;
        s = s * 0.8 + 5.0 * 0.19999999;
        let mut t = TickTimes::default();
        t.record(1, 5_000_000);
        assert_eq!(t.smoothed_tick_time_millis(), s);
    }

    #[test]
    fn autosave_interval() {
        assert_eq!(compute_next_autosave_interval(20.0), 6000);
        assert_eq!(compute_next_autosave_interval(0.1), 100);
        assert_eq!(compute_next_autosave_interval(1000.0), 300_000);
        assert_eq!(sprint_ticks_per_second(999_999), 1000.0);
    }
}
