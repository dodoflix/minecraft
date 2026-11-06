//! `net.minecraft.stats`: statistics (`Stat`, `StatType`, `Stats`, `StatsCounter`, `ServerStatsCounter`).
//!
//! A `Stat<?>` is `mc_protocol::packets::play::Stat` (the stat type plus the id of the value in that type's registry;
//! it is also what `ClientboundAwardStatsPacket` carries). The `StatType`s are the vanilla registry entries
//! `minecraft:mined`/`crafted`/`used`/`broken`/`picked_up`/`dropped`/`killed`/`killed_by`/`custom`.
pub mod hash_order;
pub mod server_stats_counter;
pub mod stat;
pub mod stat_formatter;
pub mod stats;
pub mod stats_counter;

pub use server_stats_counter::ServerStatsCounter;
pub use stat::Stat;
pub use stat_formatter::StatFormatter;
pub use stats_counter::StatsCounter;

use std::sync::{Arc, Mutex, MutexGuard};

/// `PlayerList.stats` entry: the one counter of a player, shared by the list (save/remove) and the `ServerPlayer`
/// (`ServerPlayer.stats`; the player and its respawned successor see the same counter).
#[derive(Clone)]
pub struct SharedStats(Arc<Mutex<ServerStatsCounter>>);

impl SharedStats {
    pub fn new(counter: ServerStatsCounter) -> Self {
        Self(Arc::new(Mutex::new(counter)))
    }

    pub fn lock(&self) -> MutexGuard<'_, ServerStatsCounter> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
