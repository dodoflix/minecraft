//! Level-side helpers of `Entity` base behaviour: environment flags and the portal hand-off queue.

use super::ServerLevel;
use crate::world::attribute::AttributeEnv;
use crate::world::entity::EntityId;

impl ServerLevel {
    /// `environmentAttributes().getDimensionValue(EnvironmentAttributes.FAST_LAVA)`.
    pub fn fast_lava(&self) -> bool {
        // `gameplay/fast_lava` is a dimension constant (no clock / weather input), the cached tick value is used when present
        let env = AttributeEnv { clock_total_ticks: &|_| 0, rain_level: 0.0, thunder_level: 0.0 };
        self.tick_state.attributes.get_bool("gameplay/fast_lava", &env)
    }

    /// An entity's `PortalProcessor` reached its transition time: the level tick resolves the destination with the
    /// server's other levels after the entity's tick.
    pub fn queue_portal_teleport(&mut self, id: EntityId) {
        if !self.tick_state.pending_portal_teleports.contains(&id) {
            self.tick_state.pending_portal_teleports.push(id);
        }
    }

    /// Drains the queue entry of `id` (`true` when the entity was waiting for its portal teleport).
    pub fn take_portal_teleport(&mut self, id: EntityId) -> bool {
        let before = self.tick_state.pending_portal_teleports.len();
        self.tick_state.pending_portal_teleports.retain(|&e| e != id);
        self.tick_state.pending_portal_teleports.len() != before
    }
}
