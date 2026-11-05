//! `ServerPlayer.setRespawnPosition`.
use super::ServerPlayer;
use crate::world::entity::respawn::RespawnConfig;
use mc_text::Component;

impl ServerPlayer {
    /// `setRespawnPosition(respawnConfig, showMessage)`.
    pub fn set_respawn_position(&mut self, respawn_config: Option<RespawnConfig>, show_message: bool) {
        if show_message
            && let Some(config) = &respawn_config
            && !config.is_same_position(self.respawn_config.as_ref())
            && let Some(handle) = &self.handle
        {
            handle.send_system_message(&Component::translatable("block.minecraft.set_spawn", Vec::new()), false);
        }
        self.respawn_config = respawn_config;
    }
}
