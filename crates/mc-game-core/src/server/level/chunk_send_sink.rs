//! The per-player side of chunk tracking: implemented by the connection's `PlayerChunkSender` (GamePlayNet).
use mc_core::pos::ChunkPos;

pub trait ChunkSendSink: Send {
    /// `PlayerChunkSender.markChunkPendingToSend(chunk)`.
    fn mark_chunk_pending_to_send(&mut self, pos: ChunkPos);
    /// `PlayerChunkSender.dropChunk(player, pos)` (the `ForgetLevelChunk` is only sent when the player is alive).
    fn drop_chunk(&mut self, pos: ChunkPos, player_alive: bool);
    /// `PlayerChunkSender.isPending(pos)`.
    fn is_pending(&self, pos_packed: i64) -> bool;
}
