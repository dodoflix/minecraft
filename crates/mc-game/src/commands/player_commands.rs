//! The server-side glue between the command system and players: `ServerPlayer.createCommandSourceStack`,
//! `Commands.sendCommands` and `ServerCommandSuggestionsProvider`, plus `MinecraftServer.createCommandSourceStack`.

use super::{CommandEntity, CommandSource, CommandSourceKind, CommandSourceStack, ConsoleCommandSource, host};
use crate::server::MinecraftServer;
use crate::server::permissions::PermissionSet;
use crate::server::players::PlayerHandle;
use mc_core::phys::Vec2;
use mc_protocol::packets::play::{ClientboundCommandSuggestionsEntry, ClientboundCommandSuggestionsPacket};
use mc_text::Component;
use std::sync::Arc;

/// `ServerPlayer.commandSource`.
pub struct PlayerCommandSource {
    handle: PlayerHandle,
}

impl CommandSource for PlayerCommandSource {
    fn send_system_message(&self, message: &Component) {
        self.handle.send_system_message(message, false);
    }

    fn accepts_success(&self) -> bool {
        use crate::world::level::gamerules::GameRules;
        super::server_access::has_server() && super::server_access::with_server(|server| server.game_rules_ref().get(GameRules::SEND_COMMAND_FEEDBACK))
    }

    fn accepts_failure(&self) -> bool {
        true
    }

    fn should_inform_admins(&self) -> bool {
        true
    }

    fn kind(&self) -> CommandSourceKind {
        CommandSourceKind::Player(self.handle.uuid)
    }
}

/// `player.createCommandSourceStack()`; `None` when the player entity is not in its level.
pub fn create_player_command_source_stack(server: &mut MinecraftServer, player: &PlayerHandle) -> Option<CommandSourceStack> {
    let dimension = player.dimension();
    let permissions = PermissionSet::from(server.player_list.get_profile_permissions(&player.name_and_id()));
    let entity = server.level(&dimension)?.get_entity(player.id)?;
    let base = entity.base();
    let command_entity = CommandEntity {
        id: player.id,
        uuid: player.uuid,
        is_player: true,
        display_name: host::entity_display_name(server, entity),
        plain_text_name: host::entity_plain_text_name(entity),
        eye_height: base.eye_height,
        player: Some(player.clone()),
    };
    Some(CommandSourceStack::for_entity(
        Arc::new(PlayerCommandSource { handle: player.clone() }),
        base.position(),
        Vec2::new(base.x_rot(), base.y_rot()),
        dimension,
        permissions,
        command_entity,
    ))
}

/// `Commands.sendCommands(player)`: sends the command tree `permissions` may use.
pub fn send_commands(server: &mut MinecraftServer, player: &PlayerHandle, permissions: &PermissionSet) {
    let source = CommandSourceStack::compilation_context(permissions.clone());
    let root = super::command_tree::usable_root(server.commands.get_dispatcher().get_root(), &source);
    player.connection.send(&super::command_tree::commands_packet(&root));
}

/// `ServerCommandSuggestionsProvider`: at most one request is answered per tick, the newest one wins (older requests
/// are never answered).
#[derive(Default)]
pub struct ServerCommandSuggestionsProvider {
    last_request: Option<(i32, String)>,
    has_budget: bool,
}

const MAX_COMMAND_SUGGESTIONS: usize = 1000;

impl ServerCommandSuggestionsProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// `tick()` (every tick).
    pub fn tick(&mut self, server: &mut MinecraftServer, player: &PlayerHandle) {
        self.has_budget = true;
        self.try_process_request(server, player);
    }

    /// `handleCustomCommandSuggestions`: `request(command)` and send the answer when it is computed.
    pub fn request(&mut self, server: &mut MinecraftServer, player: &PlayerHandle, id: i32, command: String) {
        self.last_request = Some((id, command));
        self.try_process_request(server, player);
    }

    fn try_process_request(&mut self, server: &mut MinecraftServer, player: &PlayerHandle) {
        if !self.has_budget {
            return;
        }
        let Some((id, command)) = self.last_request.take() else { return };
        self.has_budget = false;
        let Some(source) = create_player_command_source_stack(server, player) else { return };
        let commands = server.commands.clone();
        let packet = super::server_access::enter_server(server, || {
            let mut reader = mc_brigadier::StringReader::new(command);
            if mc_brigadier::ImmutableStringReader::can_read(&reader) && mc_brigadier::ImmutableStringReader::peek(&reader) == '/' {
                reader.skip();
            }
            let parse = commands.get_dispatcher().parse_reader(reader, source);
            let suggestions = commands.get_dispatcher().get_completion_suggestions(&parse);
            let range = suggestions.get_range();
            ClientboundCommandSuggestionsPacket {
                id,
                start: range.get_start() as i32,
                length: range.get_length() as i32,
                suggestions: suggestions
                    .get_list()
                    .iter()
                    .take(MAX_COMMAND_SUGGESTIONS)
                    .map(|s| ClientboundCommandSuggestionsEntry {
                        text: s.get_text().to_owned(),
                        tooltip: s.get_tooltip().map(|m| super::message::from_message(m.as_ref())),
                    })
                    .collect(),
            }
        });
        player.connection.send(&packet);
    }
}

