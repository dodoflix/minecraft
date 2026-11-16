//! `ServerLevel.getEntityInAnyDimension(uuid)` for code that runs inside one level.
//!
//! A level is taken out of the server while it ticks and its entities are taken out of their level while their own code
//! runs, so the other levels (and their entities) cannot be reached from there without aliasing a `&mut`. The
//! directory is a server-wide index of every tracked entity (uuid -> level, id and the state other levels read) that
//! each level keeps current from its own entity callbacks: published when an entity starts being tracked, refreshed
//! whenever the level gets the entity back from its owner's code, withdrawn when tracking ends. It never hands out an
//! entity, only a [`RemoteEntity`] copy; code that has to act on the entity queues a server action
//! ([`ServerLevel::queue_server_action`](super::ServerLevel::queue_server_action)).
//!
//! shortcut: a copy is as old as the last time its level got the entity back (end of its tick or of the packet that
//! changed it); vanilla reads the live object. Upgrade when a reader needs a value that changes between two
//! `put_back_entity` calls.

use crate::advancements::SharedAdvancements;
use crate::stats::SharedStats;
use crate::world::damagesource::DamageEntity;
use crate::world::entity::attributes::AttributeMap;
use crate::world::entity::entity::{self, DATA_SHARED_FLAGS_ID};
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::item::ItemStack;
use crate::world::level::DimensionKey;
use mc_core::phys::{AABB, Vec3};
use mc_data::EntityType;
use mc_data::enums::{GameType, Pose};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::Input;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// What other levels know about an entity of one level.
#[derive(Clone, Debug)]
pub struct RemoteEntity {
    pub uuid: Uuid,
    pub id: EntityId,
    /// The level the entity is tracked in.
    pub dimension: DimensionKey,
    pub entity_type: EntityType,
    pub position: Vec3,
    pub y_rot: f32,
    pub x_rot: f32,
    pub bounding_box: AABB,
    /// `isAlive()`.
    pub alive: bool,
    pub is_player: bool,
    /// `instanceof LivingEntity`.
    pub is_living: bool,
    pub spectator: bool,
    /// `canUsePortal(true)` (a living entity that sleeps cannot).
    pub can_use_portal: bool,
    /// `ServerPlayer.wonGame` / `seenCredits` (false for other entities).
    pub won_game: bool,
    pub seen_credits: bool,
    /// `eyeHeight` (`getEyeY()` is `position.y + eyeHeight`).
    pub eye_height: f32,
    /// `getHealth()` of a living entity.
    pub health: f32,
    /// The game mode of a player (survival for the others).
    pub game_type: GameType,
    /// `getPose()` and the shared flags byte (`isCrouching`, `isInvisible`, ...).
    pub pose: Pose,
    pub shared_flags: i8,
    /// What a stand-in of a player needs beyond the fields above (`None` for the other entities).
    pub player: Option<Arc<RemotePlayer>>,
    /// The aim, input and movement of a player that the copy keeps current with every refresh.
    pub y_head_rot: f32,
    pub y_body_rot: f32,
    pub last_client_input: Input,
    pub last_known_client_movement: Vec3,
    pub was_touching_water: bool,
    pub tick_count: i32,
    /// `getScoreboardName()`.
    pub scoreboard_name: String,
    /// `instanceof OwnableEntity` and its `getOwnerReference()`.
    pub is_ownable: bool,
    pub ownable_owner: Option<Uuid>,
    /// What a damage source keeps of a living entity (`None` for the others): taken when the entity is published and
    /// refreshed with every copy of a player (the others keep their equipment, name and aim of that moment).
    pub damage_entity: Option<Arc<DamageEntity>>,
    /// The filled maps in the inventory of a player, and whether it wears a map invisibility item (what `tickCarriedBy`
    /// reads of a holder in another level).
    pub held_maps: Arc<[ItemStack]>,
    pub map_invisibility: bool,
}

/// The state of a player that code of another level reads through its stand-in: the saved player data (equipment,
/// inventory, effects, food, experience, abilities, tags, recipes, ...), the attribute map with the transient
/// modifiers the save leaves out, and the statistics and advancements the player predicates read.
pub struct RemotePlayer {
    pub saved: CompoundTag,
    /// Behind a lock because an attribute instance caches its value in a `Cell`, which a shared `Arc` must not expose.
    pub attributes: Mutex<AttributeMap>,
    pub stats: SharedStats,
    pub advancements: SharedAdvancements,
    /// `tickCount` of the player when this was taken.
    pub tick_count: i32,
}

impl RemotePlayer {
    fn of(player: &ServerPlayer) -> Self {
        let mut saved = CompoundTag::new();
        // the generic save: the vehicle and the ender pearls of a player in another level cannot be looked up from here
        entity::save_without_id(player, &mut saved, &|_| None);
        Self {
            saved,
            attributes: Mutex::new(player.player.living.attributes.clone()),
            stats: player.stats.clone(),
            advancements: player.advancements.clone(),
            tick_count: player.base().tick_count,
        }
    }
}

impl std::fmt::Debug for RemotePlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemotePlayer").finish_non_exhaustive()
    }
}

impl RemoteEntity {
    fn of(entity: &dyn Entity, dimension: &DimensionKey) -> Self {
        let mut remote = Self {
            uuid: entity.uuid(),
            id: entity.id(),
            dimension: dimension.clone(),
            entity_type: entity.get_type(),
            position: Vec3::ZERO,
            y_rot: 0.0,
            x_rot: 0.0,
            bounding_box: entity.base().bounding_box(),
            alive: false,
            is_player: false,
            is_living: false,
            spectator: false,
            can_use_portal: false,
            won_game: false,
            seen_credits: false,
            eye_height: 0.0,
            health: 0.0,
            game_type: GameType::Survival,
            pose: Pose::Standing,
            shared_flags: 0,
            player: None,
            y_head_rot: 0.0,
            y_body_rot: 0.0,
            last_client_input: Input::default(),
            last_known_client_movement: Vec3::ZERO,
            was_touching_water: false,
            tick_count: 0,
            scoreboard_name: entity.get_scoreboard_name(),
            is_ownable: false,
            ownable_owner: None,
            damage_entity: entity
                .living()
                .is_some()
                .then(|| Arc::new(DamageEntity::of(entity))),
            held_maps: Arc::new([]),
            map_invisibility: false,
        };
        remote.update(entity);
        remote
    }

    /// Reads everything but the identity and the level from `entity`.
    fn update(&mut self, entity: &dyn Entity) {
        let player = entity.as_any().downcast_ref::<ServerPlayer>();
        let base = entity.base();
        self.position = base.position();
        self.y_rot = base.y_rot();
        self.x_rot = base.x_rot();
        self.bounding_box = base.bounding_box();
        self.alive = entity.is_alive();
        self.is_player = entity.is_player();
        self.is_living = entity.living().is_some();
        self.spectator = entity.is_spectator();
        self.can_use_portal = entity.can_use_portal(true) && !entity.is_sleeping();
        self.won_game = player.is_some_and(|p| p.won_game);
        self.seen_credits = player.is_some_and(|p| p.seen_credits);
        self.eye_height = base.eye_height;
        self.health = entity.living().map_or(0.0, |l| l.get_health());
        self.game_type = player.map_or(GameType::Survival, |p| p.game_mode.game_mode_for_player);
        self.pose = base.get_pose();
        self.shared_flags = base.entity_data.get(DATA_SHARED_FLAGS_ID);
        self.is_ownable = entity.is_ownable();
        self.ownable_owner = entity.ownable_owner_uuid();
        self.tick_count = base.tick_count;
        self.was_touching_water = base.was_touching_water;
        self.y_head_rot = entity.get_y_head_rot();
        self.y_body_rot = entity.living().map_or(0.0, |l| l.y_body_rot);
        if let Some(player) = player {
            self.damage_entity = Some(Arc::new(DamageEntity::of(entity)));
            self.last_client_input = player.last_client_input;
            self.last_known_client_movement = player.last_known_client_movement;
            // shortcut: the saved data is as old as the player's last tick step, not of whatever changed it since (a
            // packet, a hit, a command); the save is too heavy to repeat for every put-back. Upgrade when a reader
            // needs such a change before the player's next tick.
            if self
                .player
                .as_ref()
                .is_none_or(|p| p.tick_count != base.tick_count)
            {
                self.player = Some(Arc::new(RemotePlayer::of(player)));
            }
        }
        if let Some(data) = entity.player() {
            let (maps, invisibility) =
                crate::world::item::map_item::held_maps_and_invisibility(data);
            (self.held_maps, self.map_invisibility) = (maps.into(), invisibility);
        }
    }

    /// `getEyeY()`.
    pub fn eye_y(&self) -> f64 {
        self.position.y + f64::from(self.eye_height)
    }

    /// `blockPosition()`.
    pub fn block_position(&self) -> mc_core::pos::BlockPos {
        mc_core::pos::BlockPos::containing(self.position.x, self.position.y, self.position.z)
    }
}

#[derive(Default)]
struct Entries {
    by_uuid: HashMap<Uuid, RemoteEntity>,
    /// Entity ids are unique across the levels of a server.
    uuid_by_id: HashMap<EntityId, Uuid>,
}

/// The server-wide index, shared by every level of a server.
#[derive(Clone, Default)]
pub struct EntityDirectory(Arc<Mutex<Entries>>);

impl EntityDirectory {
    /// `entity` of `dimension` became visible (`EntityLookup.add`).
    pub(super) fn publish(&self, entity: &dyn Entity, dimension: &DimensionKey) {
        let entry = RemoteEntity::of(entity, dimension);
        let mut entries = self.0.lock();
        entries.uuid_by_id.insert(entry.id, entry.uuid);
        entries.by_uuid.insert(entry.uuid, entry);
    }

    /// `entity` is back in its level: bring its copy up to date (nothing when it is not published there).
    pub(super) fn refresh(&self, entity: &dyn Entity, dimension: &DimensionKey) {
        let mut entries = self.0.lock();
        if let Some(entry) = entries.by_uuid.get_mut(&entity.uuid())
            && entry.id == entity.id()
            && entry.dimension == *dimension
        {
            entry.update(entity);
        }
    }

    /// Entity `id` stopped being visible in `dimension` (`EntityLookup.remove`); a copy that already moved on to another
    /// level (a player or an entity that was added there first) stays.
    pub(super) fn withdraw(&self, id: EntityId, dimension: &DimensionKey) {
        let mut entries = self.0.lock();
        let Some(&uuid) = entries.uuid_by_id.get(&id) else {
            return;
        };
        match entries
            .by_uuid
            .get(&uuid)
            .map(|e| (e.id == id, e.dimension == *dimension))
        {
            Some((true, true)) => {
                entries.by_uuid.remove(&uuid);
                entries.uuid_by_id.remove(&id);
            }
            Some((true, false)) => {}
            _ => {
                entries.uuid_by_id.remove(&id);
            }
        }
    }

    /// The copy of the entity with `id` of whichever level tracks it.
    pub fn get_by_id(&self, id: EntityId) -> Option<RemoteEntity> {
        let entries = self.0.lock();
        entries.by_uuid.get(entries.uuid_by_id.get(&id)?).cloned()
    }

    /// The copy of the entity `uuid` of whichever level tracks it.
    pub fn get(&self, uuid: Uuid) -> Option<RemoteEntity> {
        self.0.lock().by_uuid.get(&uuid).cloned()
    }

    /// `f` on the copy of the entity `uuid`, without cloning it.
    pub fn with<R>(&self, uuid: Uuid, f: impl FnOnce(&RemoteEntity) -> R) -> Option<R> {
        self.0.lock().by_uuid.get(&uuid).map(f)
    }

    pub fn len(&self) -> usize {
        self.0.lock().by_uuid.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::level::ServerLevel;
    use crate::world::entity::factory::{self, EntitySpawnRequest};
    use mc_core::ident::Identifier;
    use mc_data::enums::EntitySpawnReason;

    fn level(name: &str, directory: &EntityDirectory) -> ServerLevel {
        let mut level = ServerLevel::detached(Identifier::parse(name).unwrap());
        level.server_state.entities = directory.clone();
        level
    }

    fn pig(level: &mut ServerLevel) -> (Uuid, EntityId) {
        let mut pig = factory::create(
            EntityType::PIG,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Command,
                ignore_checks: true,
            },
        )
        .expect("pig");
        pig.snap_to(1.5, 70.0, 2.5, 90.0, 0.0);
        let (uuid, id) = (pig.uuid(), pig.id());
        level.entity_manager.update_chunk_status(
            mc_core::pos::chunk_pos::ChunkPos::new(0, 0),
            crate::world::level::entity::Visibility::Ticking,
        );
        assert!(level.add_fresh_entity(pig));
        (uuid, id)
    }

    /// A level finds the entity of another level through the shared directory while that level's own entity is out
    /// of its slot, and a level never reports its own entities as remote.
    #[test]
    fn levels_see_each_others_entities() {
        let directory = EntityDirectory::default();
        let mut overworld = level("minecraft:overworld", &directory);
        let mut nether = level("minecraft:the_nether", &directory);
        let (uuid, id) = pig(&mut nether);
        let remote = directory.get(uuid).expect("published");
        assert_eq!(
            (remote.id, remote.dimension.to_string(), remote.alive),
            (id, "minecraft:the_nether".to_owned(), true)
        );
        assert_eq!(remote.position, Vec3::new(1.5, 70.0, 2.5));
        assert!(overworld.get_entity_by_uuid(uuid).is_none());
        assert!(matches!(
            overworld.get_entity_in_any_dimension(uuid),
            Some(super::super::AnyDimensionEntity::Elsewhere(r)) if r.id == id
        ));
        assert!(matches!(
            nether.get_entity_in_any_dimension(uuid),
            Some(super::super::AnyDimensionEntity::Here(e)) if e.id() == id
        ));
        // the entity is out of its slot (its own code runs): the copy stays, the level does not see it as its own
        let taken = nether.take_entity(id).expect("taken out");
        assert!(directory.get(uuid).is_some());
        assert!(nether.get_entity_in_any_dimension(uuid).is_none());
        nether.put_back_entity(id, taken);
        let _ = &mut overworld;
    }

    /// A copy is refreshed when its entity is put back, and withdrawn once the entity is removed.
    #[test]
    fn copies_follow_the_entity_and_leave_with_it() {
        let directory = EntityDirectory::default();
        let mut level = level("minecraft:the_end", &directory);
        let (uuid, id) = pig(&mut level);
        level.with_entity(id, |_, e| e.snap_to(8.5, 71.0, 9.5, 0.0, 0.0));
        assert_eq!(
            directory.get(uuid).unwrap().position,
            Vec3::new(8.5, 71.0, 9.5)
        );
        level.with_entity(id, |level, e| {
            level.remove_held_entity(e, crate::world::entity::RemovalReason::Discarded)
        });
        assert!(directory.get(uuid).is_none());
        assert!(directory.is_empty());
    }

    /// A copy that moved to another level before the old level's tracking-end ran survives that event.
    #[test]
    fn withdrawing_the_old_level_keeps_the_new_copy() {
        let directory = EntityDirectory::default();
        let overworld = Identifier::parse("minecraft:overworld").unwrap();
        let nether = Identifier::parse("minecraft:the_nether").unwrap();
        let mut a = level("minecraft:overworld", &directory);
        let (uuid, id) = pig(&mut a);
        let entity = a.take_entity(id).unwrap();
        directory.publish(&*entity, &nether);
        directory.withdraw(id, &overworld);
        assert_eq!(directory.get(uuid).unwrap().dimension, nether);
    }
}
