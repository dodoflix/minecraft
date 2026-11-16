//! Players of other levels that a mob of this level holds as its target.
//!
//! A mob keeps the `Player` object it hunts when that player is in another level (`Mob.target` is the entity, and a
//! saved `angry_at` is resolved in any dimension): its goals read the player's position, box, game mode and health, and
//! walk toward the coordinates. Rust mobs hold the target as an id that every goal resolves in the running level, so
//! the level keeps a stand-in for such a player: a `ServerPlayer` without a connection that mirrors what the
//! [`EntityDirectory`](super::EntityDirectory) copy knows, refreshed at the start of every tick of the level. The
//! stand-in is found by [`ServerLevel::get_entity`](super::ServerLevel::get_entity) like any entity of the level, but
//! it is no member of the level: nothing ticks, tracks, saves or lends it, and `with_entity` does not find it.
//!
//! Not implemented: an attack on the stand-in (`Mob.doHurtTarget` of vanilla hurts the real player in its level when the
//! coordinates are within reach); `hasLineOfSight` is false across levels like in vanilla.

use super::entity_directory::{EntityDirectory, RemoteEntity};
use crate::world::entity::entity::{DATA_POSE, DATA_SHARED_FLAGS_ID};
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::level::DimensionKey;
use mc_protocol::codec::GameProfile;
use mc_protocol::packets::common::ClientInformation;
use parking_lot::Mutex;
use std::collections::HashMap;

#[derive(Default)]
pub struct RemoteTargets {
    /// Stand-ins by entity id. An entry is only replaced or removed through `&mut self`, so the box a shared borrow
    /// handed out stays where it is while that borrow lives.
    stand_ins: Mutex<HashMap<EntityId, Box<ServerPlayer>>>,
}

impl RemoteTargets {
    /// The stand-in with `id`, if the level keeps one.
    pub fn get(&self, id: EntityId) -> Option<&dyn Entity> {
        let stand_ins = self.stand_ins.lock();
        let stand_in: *const ServerPlayer = &**stand_ins.get(&id)?;
        // SAFETY: the box is owned by the map and only dropped through `&mut self` (`refresh`), which cannot
        // run while the `&self` this reference borrows from is alive; inserting into the map moves boxes, not their contents.
        Some(unsafe { &*stand_in })
    }

    pub fn contains(&self, id: EntityId) -> bool {
        self.stand_ins.lock().contains_key(&id)
    }

    /// Makes sure a stand-in exists for the player `remote` (a player of another level), up to date as of its copy.
    pub fn ensure(&self, remote: &RemoteEntity) {
        self.stand_ins.lock().entry(remote.id).or_insert_with(|| {
            let profile = GameProfile {
                id: remote.uuid,
                name: remote.scoreboard_name.clone(),
                properties: Vec::new(),
            };
            // vanilla creates no entity here: the seeds of the entities created next stay the oracle's
            let mut player = mc_core::random::without_advancing_oracle_uniquifier(|| {
                Box::new(ServerPlayer::new(
                    remote.id,
                    remote.dimension.clone(),
                    profile,
                    ClientInformation::default(),
                ))
            });
            mirror(&mut player, remote);
            player
        });
    }

    /// Brings every stand-in up to date with the directory; one whose player is gone or came to `dimension` is dropped
    /// (a player that came is the entity of the level itself, with the same id).
    pub fn refresh(&mut self, directory: &EntityDirectory, dimension: &DimensionKey) {
        self.stand_ins.get_mut().retain(|_, stand_in| {
            directory
                .with(stand_in.uuid(), |remote| {
                    if remote.dimension == *dimension || remote.id != stand_in.id() {
                        return false;
                    }
                    mirror(stand_in, remote);
                    true
                })
                .unwrap_or(false)
        });
    }
}

/// What the directory copy `remote` knows of a player, written into its stand-in.
fn mirror(stand_in: &mut ServerPlayer, remote: &RemoteEntity) {
    stand_in.dimension = remote.dimension.clone();
    stand_in.game_mode.game_mode_for_player = remote.game_type;
    let base = stand_in.base_mut();
    base.set_pos_raw(remote.position.x, remote.position.y, remote.position.z);
    base.set_bounding_box(remote.bounding_box);
    base.set_y_rot(remote.y_rot);
    base.set_x_rot(remote.x_rot);
    base.eye_height = remote.eye_height;
    base.entity_data.set(DATA_POSE, remote.pose);
    base.entity_data
        .set(DATA_SHARED_FLAGS_ID, remote.shared_flags);
    if let Some(living) = stand_in.living_mut() {
        living.set_health(remote.health);
        living
            .attributes
            .get_instance(mc_data::Attribute::LUCK)
            .expect("a player has luck")
            .set_base_value(remote.luck);
    }
}
