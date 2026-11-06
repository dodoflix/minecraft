//! `ServerPlayer.registerAndUpdateEnderPearlTicket` / `placeEnderPearlTicket`: the chunk loading tickets that keep an
//! in-flight ender pearl's surroundings loaded. The pearl entity (projectile code) calls these.

use crate::server::level::{ServerLevel, TicketType};
use mc_core::pos::ChunkPos;

/// `ServerPlayer.ENDER_PEARL_TICKET_RADIUS`.
pub const ENDER_PEARL_TICKET_RADIUS: i32 = 2;

/// `ServerPlayer.placeEnderPearlTicket(level, chunk)`: returns `TicketType.ENDER_PEARL.timeout()`.
pub fn place_ender_pearl_ticket(level: &mut ServerLevel, chunk: ChunkPos) -> i64 {
    level.add_ticket_with_radius(TicketType::ENDER_PEARL, chunk, ENDER_PEARL_TICKET_RADIUS);
    TicketType::ENDER_PEARL.timeout
}

/// The level part of `registerAndUpdateEnderPearlTicket(pearl)` (the pearl set of the owner is the caller's):
/// `level.resetEmptyTime()`, then the ticket; returns the ticks until the pearl must refresh it.
pub fn register_and_update_ender_pearl_ticket(
    level: &mut ServerLevel,
    pearl_chunk: ChunkPos,
) -> i64 {
    level.reset_empty_time();
    place_ender_pearl_ticket(level, pearl_chunk) - 1
}
