//! `net.minecraft.world.clock`: `WorldClock`, `ClockState`, `ClockTimeMarker`, `ServerClockManager`.
//!
//! World clocks are identified by their registry id (`minecraft:overworld`, `minecraft:the_end`, data-pack ones);
//! a vanilla `Holder<WorldClock>` is an [`Identifier`] here. Time markers come from the timeline registry:
//! [`time_markers_from_timeline_json`] reads them out of the raw timeline JSON.
mod clock_state;
mod clock_time_marker;
mod server_clock_manager;

pub use clock_state::{ClockNetworkState, ClockState, PackedClockStates};
pub use clock_time_marker::{ClockTimeMarker, ClockTimeMarkers, time_markers_from_timeline_json};
pub use server_clock_manager::{
    ClockHost, ClockInstance, ClockTotals, MoveResult, ServerClockInstance, ServerClockManager,
};

/// `WorldClocks`.
pub mod world_clocks {
    pub const OVERWORLD: &str = "minecraft:overworld";
    pub const THE_END: &str = "minecraft:the_end";
}
