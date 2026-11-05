//! `ServerPlayer.setRespawnPosition` / `copyRespawnPosition` and `RespawnAnchorBlock.canSetSpawn`'s attribute read.

use crate::server::level::ServerLevel;
use crate::world::attribute::AttributeEnv;
use crate::world::entity::ServerPlayer;
use crate::world::entity::respawn::RespawnConfig;
use mc_core::pos::BlockPos;
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

impl ServerLevel {
    /// `environmentAttributes().getValue(EnvironmentAttributes.RESPAWN_ANCHOR_WORKS, pos)` (`canSetSpawn`): a dimension
    /// constant in vanilla's data.
    pub fn respawn_anchor_works(&self, _pos: BlockPos) -> bool {
        let env = AttributeEnv { clock_total_ticks: &|_| 0, rain_level: 0.0, thunder_level: 0.0 };
        self.tick_state.attributes.get_bool("gameplay/respawn_anchor_works", &env)
    }
}
