//! The listener's side of command execution: the player's `CommandSource` and `createCommandSourceStack()`.

use super::PlayCtx;
use crate::commands::command_source::{CommandSource, CommandSourceKind};
use crate::commands::command_source_stack::{CommandEntity, CommandSourceStack};
use crate::server::permissions::PermissionSet;
use crate::server::players::PlayerHandle;
use crate::world::entity::Entity;
use mc_core::phys::Vec2;
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
    pub(super) fn create_command_source_stack(&mut self) -> Option<CommandSourceStack> {
        let handle = self.player.handle.clone()?;
        let accepts_success = self.server.game_rule(crate::world::level::gamerules::GameRules::SEND_COMMAND_FEEDBACK);
        let id = self.player.name_and_id();
        let permissions: PermissionSet = self.server.player_list.get_profile_permissions(&id).into();
        let source = Arc::new(PlayerCommandSource { handle: handle.clone(), accepts_success });
        let entity = CommandEntity {
            id: self.player.id(),
            uuid: self.player.uuid(),
            is_player: true,
            display_name: self.player.get_display_name(),
            plain_text_name: self.player.get_plain_text_name(),
            eye_height: self.player.player.living.entity.eye_height,
            player: Some(handle),
        };
        let position = self.player.position();
        Some(CommandSourceStack::for_entity(
            source,
            position,
            Vec2::new(self.player.get_x_rot(), self.player.get_y_rot()),
            self.level.dimension.clone(),
            permissions,
            entity,
        ))
    }
}
