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

/// `(MAX_COMMAND_SEQUENCE_LENGTH, MAX_COMMAND_FORKS)` of the server's game rules.
/// SEAM(GameServerState/GameLifecycle): `MinecraftServer` does not own `GameRules` yet; vanilla's defaults (65536 each)
/// apply until it does.
pub fn command_limits(server: &mut MinecraftServer) -> (i32, i32) {
    let _ = server;
    (65536, 65536)
}

/// `level.enabledFeatures()` (SEAM: ServerLevel).
pub fn level_enabled_features(server: &mut MinecraftServer, level: &DimensionKey) -> mc_world::level_storage::FeatureFlagSet {
    let _ = (server, level);
    todo!("SEAM: ServerLevel::enabled_features")
}

/// The tail of `CommandSourceStack.broadcastToAdmins`: when `sendCommandFeedback` is on, every op player whose command
/// source is not `source_kind` gets `message`; unless the source is the server (`source_kind`), `logAdminCommands`
/// logs it to the console.
/// SEAM(GamePlayerList, GameServerState): needs the player list, `isOp`, and the game rules.
pub fn broadcast_to_admins(
    server: &mut MinecraftServer,
    source_kind: super::CommandSourceKind,
    level: &DimensionKey,
    message: &mc_text::Component,
) {
    let _ = (server, source_kind, level, message);
    todo!("SEAM: PlayerList + GameRules")
}
