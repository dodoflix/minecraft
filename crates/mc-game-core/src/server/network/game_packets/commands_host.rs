//! The listener's side of command execution: the player's `CommandSource` and `createCommandSourceStack()`.

use super::PlayCtx;
use crate::commands::command_source::{CommandSource, CommandSourceKind};
use crate::commands::command_source_stack::{CommandEntity, CommandSourceStack};
use crate::server::MinecraftServer;
use crate::server::permissions::PermissionSet;
use crate::server::players::PlayerHandle;
use crate::world::entity::Entity;
use crate::world::entity::player::ServerPlayer;
use crate::world::level::DimensionKey;
use mc_core::phys::{Vec2, Vec3};
use mc_text::Component;
use std::sync::Arc;

/// `ServerPlayer.commandSource()`.
struct PlayerCommandSource {
    handle: PlayerHandle,
    accepts_success: bool,
}

impl CommandSource for PlayerCommandSource {
    fn send_system_message(&self, message: &Component) {
        self.handle.send_system_message(message, false);
    }
    fn accepts_success(&self) -> bool {
        self.accepts_success
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

impl PlayCtx<'_> {
    /// `player.createCommandSourceStack()`.
    pub(crate) fn create_command_source_stack(&mut self) -> Option<CommandSourceStack> {
        let position = self.player.position();
        let rotation = Vec2::new(self.player.get_x_rot(), self.player.get_y_rot());
        player_command_source_stack(
            self.server,
            &self.level.dimension,
            self.player,
            position,
            rotation,
        )
    }
}

/// `player.createCommandSourceStack()` for a player that is not the context's own, at `position`/`rotation`.
pub(crate) fn player_command_source_stack(
    server: &mut MinecraftServer,
    dimension: &DimensionKey,
    player: &ServerPlayer,
    position: Vec3,
    rotation: Vec2,
) -> Option<CommandSourceStack> {
    let handle = player.handle.clone()?;
    let accepts_success =
        server.game_rule(crate::world::level::gamerules::GameRules::SEND_COMMAND_FEEDBACK);
    let id = player.name_and_id();
    let permissions: PermissionSet = server.player_list.get_profile_permissions(&id).into();
    let source = Arc::new(PlayerCommandSource {
        handle: handle.clone(),
        accepts_success,
    });
    let entity = CommandEntity {
        id: player.id(),
        uuid: player.uuid(),
        is_player: true,
        display_name: crate::commands::host::entity_display_name(server, player),
        plain_text_name: player.get_plain_text_name(),
        eye_height: player.player.living.entity.eye_height,
        player: Some(handle),
    };
    Some(CommandSourceStack::for_entity(
        source,
        position,
        rotation,
        dimension.clone(),
        permissions,
        entity,
    ))
}
