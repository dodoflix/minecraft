//! `Commands.sendCommands(player)`; the player's own command source and the suggestion provider live with the play
//! listener (`server::network::game_packets::commands_host`).

use super::CommandSourceStack;
use crate::server::MinecraftServer;
use crate::server::permissions::PermissionSet;
use crate::server::players::PlayerHandle;

/// `Commands.sendCommands(player)`: sends the command tree `permissions` may use.
pub fn send_commands(server: &mut MinecraftServer, player: &PlayerHandle, permissions: &PermissionSet) {
    let source = CommandSourceStack::compilation_context(permissions.clone());
    let root = super::command_tree::usable_root(server.commands.get_dispatcher().get_root(), &source);
    player.connection.send(&super::command_tree::commands_packet(&root));
}
