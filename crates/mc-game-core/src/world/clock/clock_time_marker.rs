//! `ClockTimeMarker` and the timeline JSON reader that feeds it.

use mc_core::ident::Identifier;
use serde_json::Value;

/// `ClockTimeMarker`: a named time (`ticks`, optionally repeating every `period_ticks`) on a clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockTimeMarker {
    pub clock: Identifier,
    pub ticks: i32,
    pub period_ticks: Option<i32>,
    pub show_in_commands: bool,
}

impl ClockTimeMarker {
    pub fn get_repetition_count(&self, total_ticks: i64) -> i64 {
        match self.period_ticks {
            None => i64::from(total_ticks >= i64::from(self.ticks)),
            Some(period) => {
                let period = i64::from(period);
                total_ticks.wrapping_div(period)
                    + i64::from(total_ticks.wrapping_rem(period) >= i64::from(self.ticks))
            }
        }
    }

    pub fn resolve_time_to_move_to(&self, total_ticks: i64) -> i64 {
        match self.period_ticks {
            None => i64::from(self.ticks),
            Some(period) => {
                let period = i64::from(period);
                total_ticks.wrapping_add(duration_to_next(
                    period,
                    total_ticks.wrapping_rem(period),
                    i64::from(self.ticks),
                ))
            }
        }
    }

    pub fn occurs_at(&self, total_ticks: i64) -> bool {
        match self.period_ticks {
            None => i64::from(self.ticks) == total_ticks,
            Some(period) => i64::from(self.ticks) == total_ticks.wrapping_rem(i64::from(period)),
        }
    }
}

fn duration_to_next(period_ticks: i64, from: i64, to: i64) -> i64 {
    let duration = to - from;
    if duration > 0 {
        duration
    } else {
        period_ticks + duration
    }
}

/// `ClockTimeMarkers` keys (`minecraft:day`, ...).
pub struct ClockTimeMarkers;

impl ClockTimeMarkers {
    pub const DAY: &'static str = "minecraft:day";
    pub const NOON: &'static str = "minecraft:noon";
    pub const NIGHT: &'static str = "minecraft:night";
    pub const MIDNIGHT: &'static str = "minecraft:midnight";
    pub const WAKE_UP_FROM_SLEEP: &'static str = "minecraft:wake_up_from_sleep";
    pub const ROLL_VILLAGE_SIEGE: &'static str = "minecraft:roll_village_siege";
}

/// `Timeline.registerTimeMarkers` over one raw timeline JSON object (`clock`, `period_ticks`, `time_markers`).
/// A marker is `{ticks: int >= 0, show_in_commands: bool = false}` or a bare int (`ticks`). Mirrors
/// `Timeline.DIRECT_CODEC`'s validation of those fields; returns `(marker id, marker)` in file order.
pub fn time_markers_from_timeline_json(
    timeline: &Value,
) -> Result<Vec<(Identifier, ClockTimeMarker)>, String> {
    let obj = timeline.as_object().ok_or("Not a JSON object")?;
    let clock = Identifier::read(
        obj.get("clock")
            .and_then(Value::as_str)
            .ok_or("No key clock in MapLike")?,
    )?;
    let period_ticks = match obj.get("period_ticks") {
        None => None,
        Some(v) => {
            let p = v
                .as_i64()
                .filter(|p| *p > 0 && *p <= i64::from(i32::MAX))
                .ok_or("Value must be positive: period_ticks")? as i32;
            Some(p)
        }
    };
    let mut out = Vec::new();
    if let Some(markers) = obj.get("time_markers") {
        for (key, info) in markers.as_object().ok_or("Not a map: time_markers")? {
            let id = Identifier::read(key)?;
            let (ticks, show) = match info {
                Value::Object(o) => (
                    o.get("ticks")
                        .and_then(Value::as_i64)
                        .ok_or("No key ticks in MapLike")?,
                    o.get("show_in_commands")
                        .map(|b| b.as_bool().ok_or("Not a boolean"))
                        .transpose()?
                        .unwrap_or(false),
                ),
                other => (other.as_i64().ok_or("Not a number")?, false),
            };
            if !(0..=i64::from(i32::MAX)).contains(&ticks) {
                return Err(format!("Value must be non-negative: {ticks}"));
            }
            if let Some(p) = period_ticks
                && ticks >= i64::from(p)
            {
                return Err(format!("Time Marker {id} must be in range [0; {p})"));
            }
            out.push((
                id,
                ClockTimeMarker {
                    clock: clock.clone(),
                    ticks: ticks as i32,
                    period_ticks,
                    show_in_commands: show,
                },
            ));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker(ticks: i32, period: Option<i32>) -> ClockTimeMarker {
        ClockTimeMarker {
            clock: Identifier::parse("minecraft:overworld").unwrap(),
            ticks,
            period_ticks: period,
            show_in_commands: true,
        }
    }

    #[test]
    fn marker_math() {
        let day = marker(1000, Some(24000));
        assert!(day.occurs_at(1000) && day.occurs_at(25000) && !day.occurs_at(999));
        assert_eq!(day.resolve_time_to_move_to(0), 1000);
        assert_eq!(day.resolve_time_to_move_to(1000), 25000); // vanilla: occursAt is checked before moving
        assert_eq!(day.resolve_time_to_move_to(5000), 25000);
        assert_eq!(day.get_repetition_count(999), 0);
        assert_eq!(day.get_repetition_count(1000), 1);
        assert_eq!(day.get_repetition_count(24999), 1);
        assert_eq!(day.get_repetition_count(25000), 2);
        let once = marker(100, None);
        assert_eq!(once.resolve_time_to_move_to(5), 100);
        assert_eq!(once.get_repetition_count(99), 0);
        assert_eq!(once.get_repetition_count(100), 1);
        assert!(once.occurs_at(100) && !once.occurs_at(24100));
    }

    #[test]
    fn reads_vanilla_day_timeline() {
        let json = mc_data::datapack::read_string("data/minecraft/timeline/day.json").unwrap();
        let markers =
            time_markers_from_timeline_json(&serde_json::from_str(&json).unwrap()).unwrap();
        let by: std::collections::HashMap<_, _> = markers
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        assert_eq!(by["minecraft:noon"].ticks, 6000);
        assert!(by["minecraft:noon"].show_in_commands);
        assert_eq!(
            by["minecraft:wake_up_from_sleep"],
            marker(0, Some(24000)).tap_show(false)
        );
        assert_eq!(by["minecraft:roll_village_siege"].ticks, 18000);
    }

    impl ClockTimeMarker {
        fn tap_show(mut self, show: bool) -> Self {
            self.show_in_commands = show;
            self
        }
    }
}
