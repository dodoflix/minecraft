//! `VibrationSystem.Data`.

use super::vibration_info::{self, VibrationInfo};
use super::vibration_selector::{self, VibrationSelector};
use mc_dfu::extra::{NON_NEGATIVE_INT, optional_always_present_field_of};
use mc_dfu::{Codec, record};
use std::sync::LazyLock;

#[derive(Clone, Debug, Default)]
pub struct VibrationData {
    pub current_vibration: Option<VibrationInfo>,
    pub travel_time_in_ticks: i32,
    pub selection_strategy: VibrationSelector,
    pub reload_vibration_particle: bool,
}

struct Wire {
    event: Option<VibrationInfo>,
    selector: VibrationSelector,
    event_delay: i32,
}

impl VibrationData {
    /// `VibrationSystem.Data.CODEC`.
    pub fn codec() -> &'static Codec<VibrationData> {
        static CODEC: LazyLock<Codec<VibrationData>> = LazyLock::new(|| {
            record!(Wire {
                event: vibration_info::CODEC.lenient_optional_field_of("event"),
                selector: vibration_selector::CODEC.field_of("selector"),
                event_delay: optional_always_present_field_of(&NON_NEGATIVE_INT, "event_delay", 0),
            })
            .codec()
            .xmap(
                |w: Wire| VibrationData {
                    current_vibration: w.event,
                    travel_time_in_ticks: w.event_delay,
                    selection_strategy: w.selector,
                    reload_vibration_particle: true,
                },
                |d: &VibrationData| Wire {
                    event: d.current_vibration.clone(),
                    selector: d.selection_strategy.clone(),
                    event_delay: d.travel_time_in_ticks,
                },
            )
        });
        &CODEC
    }
}
