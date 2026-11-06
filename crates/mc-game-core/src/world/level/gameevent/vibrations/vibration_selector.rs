//! `VibrationSelector`: the candidate vibration of the current tick.

use super::vibration_info::{self, VibrationInfo};
use super::vibration_system::get_game_event_frequency;
use mc_dfu::{Codec, LONG, record};
use std::sync::LazyLock;

/// `currentVibrationData`: the chosen vibration and the tick it was added.
#[derive(Clone, Debug, Default)]
pub struct VibrationSelector {
    pub current_vibration_data: Option<(VibrationInfo, i64)>,
}

impl VibrationSelector {
    /// `addCandidate(newVibration, tickTime)`.
    pub fn add_candidate(&mut self, new_vibration: VibrationInfo, tick_time: i64) {
        if self.should_replace_vibration(&new_vibration, tick_time) {
            self.current_vibration_data = Some((new_vibration, tick_time));
        }
    }

    fn should_replace_vibration(&self, new_vibration: &VibrationInfo, tick_time: i64) -> bool {
        let Some((previous, previous_tick)) = &self.current_vibration_data else {
            return true;
        };
        if tick_time != *previous_tick {
            return false;
        }
        if new_vibration.distance < previous.distance {
            true
        } else if new_vibration.distance > previous.distance {
            false
        } else {
            get_game_event_frequency(new_vibration.game_event)
                > get_game_event_frequency(previous.game_event)
        }
    }

    /// `chosenCandidate(time)`: the candidate of an earlier tick.
    pub fn chosen_candidate(&self, time: i64) -> Option<VibrationInfo> {
        match &self.current_vibration_data {
            Some((event, tick)) if *tick < time => Some(event.clone()),
            _ => None,
        }
    }

    /// `startOver()`.
    pub fn start_over(&mut self) {
        self.current_vibration_data = None;
    }
}

struct Wire {
    event: Option<VibrationInfo>,
    tick: i64,
}

/// `VibrationSelector.CODEC` (`tick` is `-1` without a vibration).
pub static CODEC: LazyLock<Codec<VibrationSelector>> = LazyLock::new(|| {
    record!(Wire {
        event: vibration_info::CODEC.lenient_optional_field_of("event"),
        tick: LONG.field_of("tick"),
    })
    .codec()
    .xmap(
        |w: Wire| VibrationSelector {
            current_vibration_data: w.event.map(|event| (event, w.tick)),
        },
        |s: &VibrationSelector| match &s.current_vibration_data {
            Some((event, tick)) => Wire {
                event: Some(event.clone()),
                tick: *tick,
            },
            None => Wire {
                event: None,
                tick: -1,
            },
        },
    )
});
