//! The level's record of one of its players: what readers that are not the player's own code need (tracker pairing,
//! sound and particle broadcasts, packet broadcasts, `getPlayerByUUID`, box queries and `isUnobstructed`), also while the
//! player is out of the entity storage because its packet or tick is being handled.

use crate::server::network::ConnectionSender;
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use mc_core::phys::{AABB, Vec3};
use uuid::Uuid;

/// A player as the level sees it from outside its own frame.
#[derive(Clone)]
pub struct PlayerRecord {
    pub id: EntityId,
    pub uuid: Uuid,
    pub position: Vec3,
    pub bounding_box: AABB,
    pub requested_view_distance: i32,
    pub connection: Option<ConnectionSender>,
    pub spectator: bool,
    pub removed: bool,
    pub blocks_building: bool,
    pub vehicle: Option<EntityId>,
}

impl PlayerRecord {
    pub fn of(player: &ServerPlayer) -> Self {
        Self {
            id: player.base_id(),
            uuid: player.uuid(),
            position: player.player.living.entity.position(),
            bounding_box: player.get_bounding_box(),
            requested_view_distance: player.requested_view_distance,
            connection: player.connection.clone(),
            spectator: player.is_spectator(),
            removed: player.is_removed(),
            blocks_building: player.base().blocks_building,
            vehicle: player.base().vehicle,
        }
    }
}
