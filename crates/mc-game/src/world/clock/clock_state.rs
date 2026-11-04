//! `ClockState`, `ClockNetworkState`, `PackedClockStates`.

use mc_core::ident::Identifier;
use mc_nbt::{CompoundTag, Tag};

use crate::util::codec::{self, DataResult, DecodeResult, map_decoded};

/// `ClockState` (persisted per clock).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockState {
    pub total_ticks: i64,
    pub partial_tick: f32,
    pub rate: f32,
    pub paused: bool,
}

impl ClockState {
    /// `ClockState.CODEC` decode. `rate` is `ExtraCodecs.POSITIVE_FLOAT` (> 0).
    pub fn decode(tag: &Tag) -> DataResult<ClockState> {
        let m = codec::compound(tag)?;
        let total_ticks = codec::long(codec::field(m, "total_ticks")?)?;
        let partial_tick = codec::optional(m, "partial_tick", codec::float)?.unwrap_or(0.0);
        let rate = match codec::optional(m, "rate", codec::float)? {
            Some(rate) if rate > 0.0 => rate,
            Some(rate) => return Err(format!("Value must be positive: {}", mc_core::math::java::java_float_to_string(rate))),
            None => 1.0,
        };
        let paused = codec::optional(m, "paused", codec::boolean)?.unwrap_or(false);
        Ok(ClockState { total_ticks, partial_tick, rate, paused })
    }

    /// `ClockState.CODEC` encode: defaults are omitted (`optionalFieldOf(name, default)`).
    pub fn encode(&self) -> Tag {
        let mut m = CompoundTag::new();
        m.put_long("total_ticks", self.total_ticks);
        // `Objects.equals(Float, Float)` compares bits (NaN equal, -0.0 != 0.0).
        if self.partial_tick.to_bits() != 0.0f32.to_bits() {
            m.put("partial_tick", Tag::float(self.partial_tick));
        }
        if self.rate.to_bits() != 1.0f32.to_bits() {
            m.put("rate", Tag::float(self.rate));
        }
        if self.paused {
            m.put_boolean("paused", true);
        }
        Tag::Compound(m)
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

    pub fn decode(tag: &Tag) -> DecodeResult<PackedClockStates> {
        map_decoded(codec::unbounded_map(tag, Identifier::read, ClockState::decode), |clocks| PackedClockStates { clocks })
    }

    pub fn encode(&self) -> Tag {
        let mut m = CompoundTag::new();
        for (id, state) in &self.clocks {
            m.put(id.to_string(), state.encode());
        }
        Tag::Compound(m)
    }
}
