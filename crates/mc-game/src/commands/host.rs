//! Seams to sibling slices' state (levels, entities, player list): every place where a command needs something owned
//! by another slice goes through here, so the adaptation to their final APIs is confined to this file.
//! Each function documents the vanilla call it stands for.

use crate::server::MinecraftServer;
use crate::world::level::DimensionKey;
use mc_core::pos::BlockPos;

/// `level.hasChunkAt(pos)` (SEAM: ServerLevel).
pub fn level_has_chunk_at(server: &mut MinecraftServer, level: &DimensionKey, pos: BlockPos) -> bool {
    let _ = (server, level, pos);
    todo!("SEAM: ServerLevel::has_chunk_at")
}

/// `level.isInWorldBounds(pos)` (SEAM: ServerLevel).
pub fn level_is_in_world_bounds(server: &mut MinecraftServer, level: &DimensionKey, pos: BlockPos) -> bool {
    let _ = (server, level, pos);
    todo!("SEAM: ServerLevel::is_in_world_bounds")
}

/// `(MAX_COMMAND_SEQUENCE_LENGTH, MAX_COMMAND_FORKS)` of the server's game rules (SEAM: GameServerState).
pub fn command_limits(server: &mut MinecraftServer) -> (i32, i32) {
    let _ = server;
    todo!("SEAM: MinecraftServer::game_rules")
}

/// `level.enabledFeatures()` (SEAM: ServerLevel).
pub fn level_enabled_features(server: &mut MinecraftServer, level: &DimensionKey) -> mc_world::level_storage::FeatureFlagSet {
    let _ = (server, level);
    todo!("SEAM: ServerLevel::enabled_features")
}
