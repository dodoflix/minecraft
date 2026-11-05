//! `VibrationSelector`: the candidate vibration of the current tick.

use super::vibration_info::{self, VibrationInfo};
use mc_dfu::{Codec, LONG, record};
use std::sync::LazyLock;

/// `currentVibrationData`: the chosen vibration and the tick it was added.
#[derive(Clone, Debug, Default)]
pub struct VibrationSelector {
    pub current_vibration_data: Option<(VibrationInfo, i64)>,
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
        |w: Wire| VibrationSelector { current_vibration_data: w.event.map(|event| (event, w.tick)) },
        |s: &VibrationSelector| match &s.current_vibration_data {
            Some((event, tick)) => Wire { event: Some(event.clone()), tick: *tick },
            None => Wire { event: None, tick: -1 },
        },
    )
});
