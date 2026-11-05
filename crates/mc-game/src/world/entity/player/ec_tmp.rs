//! TEMP (uncommitted) stand-ins for EcPlayer's API, deleted when it lands.
use super::ServerPlayer;
use crate::server::level::ServerLevel;
impl ServerPlayer {
    pub fn give_experience_points(&mut self, _level: &mut ServerLevel, amount: i32) {
        self.player.total_experience = self.player.total_experience.saturating_add(amount);
    }
}
