//! `ServerPlayer.setRespawnPosition` / `copyRespawnPosition` and `RespawnAnchorBlock.canSetSpawn`'s attribute read.

use crate::world::entity::ServerPlayer;
use crate::world::entity::respawn::RespawnConfig;
use mc_text::Component;

impl ServerPlayer {
    /// `setRespawnPosition(respawnConfig, showMessage)`: the "Respawn point set" message when the position changed.
    pub fn set_respawn_position(&mut self, respawn_config: Option<RespawnConfig>, show_message: bool) {
        if show_message
            && let Some(config) = &respawn_config
            && !config.is_same_position(self.respawn_config.as_ref())
        {
            self.send_system_message(Component::translatable("block.minecraft.set_spawn", Vec::new()));
        }
        self.respawn_config = respawn_config;
    }
}
