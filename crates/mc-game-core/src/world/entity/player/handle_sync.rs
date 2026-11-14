//! Keeps the `PlayerHandle` mirror of a `ServerPlayer` current (see `server::players::player_handle`): everything
//! another player or the player list reads off a `ServerPlayer` from outside its level.

use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity as _;

/// `PlayerModelPart.HAT.getMask()` (`1 << 6`).
const HAT_MASK: u8 = 1 << 6;

impl ServerPlayer {
    /// Writes position, dimension, game mode, tab-list state and client settings into the handle. Called at the end
    /// of `doTick`, after teleports and on game-mode changes.
    pub fn sync_handle(&self) {
        let Some(handle) = &self.handle else { return };
        let mut state = handle.shared.lock();
        state.dimension = self.dimension.clone();
        state.position = self.player.living.entity.position();
        state.game_mode = self.game_mode.game_mode_for_player;
        state.show_hat = self.player.is_model_part_shown(HAT_MASK);
        state.chat_visibility = self.chat_visibility;
        state.allows_listing = self.allows_listing;
        state.text_filtering_enabled = self.text_filtering_enabled;
    }

    /// `requestDebugSubscriptions(subscriptions)`.
    pub fn request_debug_subscriptions(&self, subscriptions: &[mc_data::DebugSubscription]) {
        if let Some(handle) = &self.handle {
            handle.shared.lock().requested_debug_subscriptions = subscriptions.to_vec();
        }
    }

    /// `debugSubscriptions()`: the requested subscriptions of an operator (`hasRequiredPermissions`), else none.
    pub fn debug_subscriptions(&self, level: &ServerLevel) -> Vec<mc_data::DebugSubscription> {
        match &self.handle {
            Some(handle) if level.is_op_uuid(self.uuid()) => {
                handle.shared.lock().requested_debug_subscriptions.clone()
            }
            _ => Vec::new(),
        }
    }
}
