//! Players of other levels that a mob of this level holds as its target.
//!
//! A mob keeps the `Player` object it hunts when that player is in another level (`Mob.target` is the entity, and a
//! saved `angry_at` is resolved in any dimension): its goals read the player's position, box, game mode and health, and
//! walk toward the coordinates. Rust mobs hold the target as an id that every goal resolves in the running level, and
//! kill credit, loot and advancement predicates read the killer of a damage source the same way, so the level keeps a
//! stand-in for such a player: a `ServerPlayer` without a connection that mirrors what the
//! [`EntityDirectory`](super::EntityDirectory) copy knows (the saved player data, so its equipment, effects, food,
//! experience and tags, and its attributes, statistics and advancements), refreshed at the start of every tick of the
//! level. The
//! stand-in is found by [`ServerLevel::get_entity_or_remote_target`](super::ServerLevel::get_entity_or_remote_target)
//! (the lookup of code that vanilla runs on the `Player` it holds), never by `get_entity`; it is no member of the level: nothing ticks, tracks, saves or lends it, and `with_entity` does not find it.
//!
//! `hasLineOfSight` is false across levels like in vanilla, and the melee goals and behaviors of the neutral mobs attack
//! only with a line of sight, so a mob never hurts the player it walks toward (checked against the real server).

use super::entity_directory::{EntityDirectory, RemoteEntity, RemotePlayer};
use crate::world::entity::entity::{DATA_POSE, DATA_SHARED_FLAGS_ID, load};
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::level::DimensionKey;
use mc_protocol::codec::GameProfile;
use mc_protocol::packets::common::ClientInformation;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default)]
pub struct RemoteTargets {
    /// Stand-ins by entity id. An entry is only replaced or removed through `&mut self`, so the box a shared borrow
    /// handed out stays where it is while that borrow lives.
    stand_ins: Mutex<HashMap<EntityId, StandIn>>,
}

struct StandIn {
    player: Box<ServerPlayer>,
    /// The saved data `player` was last loaded from: a copy that still has it is not loaded again.
    data: Option<Arc<RemotePlayer>>,
}

impl RemoteTargets {
    /// The stand-in with `id`, if the level keeps one.
    pub fn get(&self, id: EntityId) -> Option<&dyn Entity> {
        let stand_ins = self.stand_ins.lock();
        let stand_in: *const ServerPlayer = &*stand_ins.get(&id)?.player;
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
            let player = mc_core::random::without_advancing_oracle_uniquifier(|| {
                Box::new(ServerPlayer::new(
                    remote.id,
                    remote.dimension.clone(),
                    profile,
                    ClientInformation::default(),
                ))
            });
            let mut stand_in = StandIn { player, data: None };
            mirror(&mut stand_in, remote);
            stand_in
        });
    }

    /// Brings every stand-in up to date with the directory; one whose player is gone, came to `dimension` or that no
    /// reader asked for lately is dropped (a player that came is the entity of the level itself, with the same id).
    pub fn refresh(&mut self, directory: &EntityDirectory, dimension: &DimensionKey) {
        self.stand_ins.get_mut().retain(|_, stand_in| {
            directory
                .with_wanted(stand_in.player.uuid(), |remote| {
                    if remote.dimension == *dimension || remote.id != stand_in.player.id() {
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
fn mirror(stand_in: &mut StandIn, remote: &RemoteEntity) {
    let player = &mut *stand_in.player;
    // before the load: the saved data of timed state reads the player's age
    player.base_mut().tick_count = remote.tick_count;
    if let Some(data) = &remote.player {
        // the player's data first: the fields below are the live values of the copy
        if !stand_in
            .data
            .as_ref()
            .is_some_and(|loaded| Arc::ptr_eq(loaded, data))
        {
            // a recipe that was taken away must not survive the load, which only adds
            player.recipe_book = Default::default();
            let loaded = load(player, None, &data.saved);
            debug_assert!(loaded.is_ok(), "the saved data of a live player loads");
            player.player.living.attributes = data.attributes.lock().clone();
            player.stats = data.stats.clone();
            player.advancements = data.advancements.clone();
            stand_in.data = Some(data.clone());
        }
    } else {
        stand_in.data = None;
    }
    player.last_client_input = remote.last_client_input;
    player.last_known_client_movement = remote.last_known_client_movement;
    player.set_y_head_rot(remote.y_head_rot);
    player.set_y_body_rot(remote.y_body_rot);
    player.dimension = remote.dimension.clone();
    player.game_mode.game_mode_for_player = remote.game_type;
    let base = player.base_mut();
    base.set_pos_raw(remote.position.x, remote.position.y, remote.position.z);
    base.set_bounding_box(remote.bounding_box);
    base.set_y_rot(remote.y_rot);
    base.set_x_rot(remote.x_rot);
    base.eye_height = remote.eye_height;
    base.was_touching_water = remote.was_touching_water;
    base.entity_data.set(DATA_POSE, remote.pose);
    base.entity_data
        .set(DATA_SHARED_FLAGS_ID, remote.shared_flags);
    if let Some(living) = player.living_mut() {
        living.set_health(remote.health);
    }
}
