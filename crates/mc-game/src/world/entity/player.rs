//! `ServerPlayer` skeleton (EntityAgent grows it: LivingEntity/Player data, inventory, game mode, stats, ...).
use super::{Entity, EntityData};
use crate::server::network::ConnectionSender;
use mc_protocol::codec::GameProfile;
use std::any::Any;

pub struct ServerPlayer {
    pub entity: EntityData,
    pub profile: GameProfile,
    /// `None` for a player whose connection is gone / test players.
    pub connection: Option<ConnectionSender>,
}

impl Entity for ServerPlayer {
    fn base(&self) -> &EntityData {
        &self.entity
    }
    fn base_mut(&mut self) -> &mut EntityData {
        &mut self.entity
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn is_player(&self) -> bool {
        true
    }
    fn is_always_ticking(&self) -> bool {
        true
    }
}
