//! Level-side helpers of `Entity` base behaviour: environment flags.

use super::ServerLevel;

impl ServerLevel {
    /// Work that needs the server (other levels, the player list), queued by code that runs inside this level and cannot
    /// reach it (an entity's tick): the server runs it right after the entity's tick, with this level out of its slot
    /// (lend it with `MinecraftServer::with_lent_level` to let the server reach it again).
    pub fn queue_server_action(
        &mut self,
        action: impl FnOnce(&mut crate::server::MinecraftServer, &mut ServerLevel) + Send + 'static,
    ) {
        self.tick_state
            .pending_server_actions
            .push(Box::new(action));
    }

    /// Like [`ServerLevel::queue_server_action`] for work that could not run because the level it needs is out of the
    /// server (held by a caller further up the stack): it waits until the next time the server runs this level's queue.
    pub fn defer_server_action(
        &mut self,
        action: impl FnOnce(&mut crate::server::MinecraftServer, &mut ServerLevel) + Send + 'static,
    ) {
        self.tick_state
            .deferred_server_actions
            .push(Box::new(action));
    }

    pub fn has_server_actions(&self) -> bool {
        !self.tick_state.pending_server_actions.is_empty()
            || !self.tick_state.deferred_server_actions.is_empty()
    }

    /// The queued work, deferred work first.
    pub fn take_server_actions(&mut self) -> Vec<super::level_tick_state::ServerAction> {
        let mut actions = std::mem::take(&mut self.tick_state.deferred_server_actions);
        actions.append(&mut self.tick_state.pending_server_actions);
        actions
    }

    /// `environmentAttributes().getDimensionValue(EnvironmentAttributes.FAST_LAVA)`.
    pub fn fast_lava(&self) -> bool {
        self.dimension_bool(crate::world::attribute::attributes::FAST_LAVA)
    }
}
