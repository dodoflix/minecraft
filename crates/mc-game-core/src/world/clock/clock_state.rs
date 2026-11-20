//! `ClockState`, `ClockNetworkState`, `PackedClockStates`.

use std::collections::HashSet;
use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_dfu::extra::{IDENTIFIER, POSITIVE_FLOAT};
use mc_dfu::{BOOL, Codec, DataResult, FLOAT, LONG, MapCodec};

/// `ClockState` (persisted per clock).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockState {
    pub total_ticks: i64,
    pub partial_tick: f32,
    pub rate: f32,
    pub paused: bool,
}

impl ClockState {
    /// `ClockState.CODEC`: `rate` is `ExtraCodecs.POSITIVE_FLOAT`; defaults are omitted (`optionalFieldOf(name, default)`).
    pub fn codec() -> Codec<ClockState> {
        // `Objects.equals(Float, Float)` compares bits (NaN equal, -0.0 != 0.0).
        let float_or = |name: &str, default: f32| {
            let default_bits = default.to_bits();
            let field: MapCodec<f32> = FLOAT.optional_field_of(name).xmap(
                move |value| value.unwrap_or(default),
                move |value: &f32| (value.to_bits() != default_bits).then_some(*value),
            );
            field
        };
        mc_dfu::record!(ClockState {
            total_ticks: LONG.field_of("total_ticks"),
            partial_tick: float_or("partial_tick", 0.0),
            rate: {
                let default_bits = 1.0f32.to_bits();
                POSITIVE_FLOAT.optional_field_of("rate").xmap(
                    |value| value.unwrap_or(1.0),
                    move |value: &f32| (value.to_bits() != default_bits).then_some(*value),
                )
            },
            paused: BOOL.optional_field_of_default("paused", false),
        })
        .codec()
    }
}

/// `ClockNetworkState` (sent in `ClientboundSetTimePacket`; the network layer owns the stream codec).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockNetworkState {
    pub total_ticks: i64,
    pub partial_tick: f32,
    pub rate: f32,
}

/// `PackedClockStates`: `data/minecraft/world_clocks.dat`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PackedClockStates {
    pub clocks: Vec<(Identifier, ClockState)>,
}

impl PackedClockStates {
    pub const EMPTY: PackedClockStates = PackedClockStates { clocks: Vec::new() };

    /// `PackedClockStates.CODEC`: `Codec.unboundedMap(WorldClock.CODEC, ClockState.CODEC)`; the keys are holders of the
    /// `world_clock` registry, which holds `known_clocks`.
    pub fn codec(known_clocks: Arc<HashSet<Identifier>>) -> Codec<PackedClockStates> {
        let clock = IDENTIFIER.flat_xmap(
            move |id| {
                if known_clocks.contains(&id) {
                    DataResult::success(id)
                } else {
                    DataResult::error(move || {
                        format!("Failed to get element {id} from registry minecraft:world_clock")
                    })
                }
            },
            |id: &Identifier| DataResult::success(id.clone()),
        );
        clock.unbounded_map(&ClockState::codec()).xmap(
            |clocks| PackedClockStates { clocks },
            |packed: &PackedClockStates| packed.clocks.clone(),
        )
    }
}
