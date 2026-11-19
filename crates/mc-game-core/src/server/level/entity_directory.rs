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
//!
//! shortcut: the full saved state of a player is copied only while a level asks for its stand-in or a projectile or
//! primed TNT it owns exists in another level; a stand-in first asked for by anything else (a mob's target, a damage
//! source made by a command, the owner of a pet that kills) has the live fields of the copy at once and the saved state
//! (gear, effects, food) from the player's next put-back. Upgrade by naming the player wanted where such a reference is
//! created.

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
    /// `instanceof Mob`.
    pub is_mob: bool,
    pub spectator: bool,
    /// `canUsePortal(true)` (a living entity that sleeps cannot).
    pub can_use_portal: bool,
    /// `ServerPlayer.wonGame` / `seenCredits` (false for other entities).
    pub won_game: bool,
    pub seen_credits: bool,
    /// `eyeHeight` (`getEyeY()` is `position.y + eyeHeight`).
    pub eye_height: f32,
    /// `getBbHeight()`.
    pub bb_height: f32,
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
    /// Only a player has them.
    pub player_state: Option<(SharedStats, SharedAdvancements)>,
}

impl RemotePlayer {
    /// The state of a living entity (`None` for any other).
    fn of(entity: &dyn Entity) -> Option<Self> {
        let mut saved = CompoundTag::new();
        // the generic save: the vehicle and the ender pearls of an entity in another level cannot be looked up from here
        entity::save_without_id(entity, &mut saved, &|_| None);
        Some(Self {
            saved,
            attributes: Mutex::new(entity.living()?.attributes.clone()),
            player_state: entity
                .as_any()
                .downcast_ref::<ServerPlayer>()
                .map(|p| (p.stats.clone(), p.advancements.clone())),
        })
    }
}

impl std::fmt::Debug for RemotePlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemotePlayer").finish_non_exhaustive()
    }
}

impl RemoteEntity {
    fn of(entity: &dyn Entity, dimension: &DimensionKey, wanted: bool) -> Self {
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
            is_mob: false,
            spectator: false,
            can_use_portal: false,
            won_game: false,
            seen_credits: false,
            eye_height: 0.0,
            bb_height: 0.0,
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
        remote.update(entity, wanted);
        remote
    }

    /// Reads everything but the identity and the level from `entity`. `wanted`: a level reads the full state of this
    /// player (a stand-in), so the saved data is copied too (the full save is too heavy to take of every player).
    fn update(&mut self, entity: &dyn Entity, wanted: bool) {
        let player = entity.as_any().downcast_ref::<ServerPlayer>();
        let base = entity.base();
        self.position = base.position();
        self.y_rot = base.y_rot();
        self.x_rot = base.x_rot();
        self.bounding_box = base.bounding_box();
        self.alive = entity.is_alive();
        self.is_player = entity.is_player();
        self.is_living = entity.living().is_some();
        self.is_mob = entity.is_mob();
        self.spectator = entity.is_spectator();
        self.can_use_portal = entity.can_use_portal(true) && !entity.is_sleeping();
        self.won_game = player.is_some_and(|p| p.won_game);
        self.seen_credits = player.is_some_and(|p| p.seen_credits);
        self.eye_height = base.eye_height;
        self.bb_height = base.get_bb_height();
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
        }
        // the saved data is taken again with every put-back (a packet, a hit, a command change it between ticks) but
        // only of an entity a level reads
        self.player = wanted
            .then(|| RemotePlayer::of(entity).map(Arc::new))
            .flatten();
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

/// How many [`EntityDirectory::advance_clock`] steps a player stays wanted after the last time something asked for it:
/// every level steps the clock once per tick, so this is at least 200 ticks (more than the 100 ticks a kill is credited
/// to its last attacker) for up to three levels.
const WANT_WINDOW: u64 = 600;

#[derive(Default)]
struct Entries {
    by_uuid: HashMap<Uuid, RemoteEntity>,
    /// Entity ids are unique across the levels of a server.
    uuid_by_id: HashMap<EntityId, Uuid>,
    /// Counts the level ticks of the server (every level steps it once per tick).
    clock: u64,
    /// The players whose full state some level reads through a stand-in or may read soon (an owned projectile or
    /// primed TNT in flight), with the clock value until which it stays so. Only their saved data is copied.
    wanted_until: HashMap<Uuid, u64>,
}

impl Entries {
    fn is_wanted(&self, uuid: &Uuid) -> bool {
        self.wanted_until
            .get(uuid)
            .is_some_and(|&t| t >= self.clock)
    }

    fn want(&mut self, uuid: Uuid) {
        self.wanted_until.insert(uuid, self.clock + WANT_WINDOW);
    }

    /// An entity that names its owner keeps the owner wanted when the owner is a living entity of another level (the
    /// level of `entity` reads it through a stand-in).
    fn want_owner_of(&mut self, entity: &dyn Entity, dimension: &DimensionKey) {
        if let Some(owner) = entity.owner_reference()
            && self
                .by_uuid
                .get(&owner)
                .is_some_and(|o| o.is_living && o.dimension != *dimension)
        {
            self.want(owner);
        }
    }
}

/// The server-wide index, shared by every level of a server.
#[derive(Clone, Default)]
pub struct EntityDirectory(Arc<Mutex<Entries>>);

impl EntityDirectory {
    /// `entity` of `dimension` became visible (`EntityLookup.add`).
    pub(super) fn publish(&self, entity: &dyn Entity, dimension: &DimensionKey) {
        let mut entries = self.0.lock();
        let wanted = entries.is_wanted(&entity.uuid());
        let entry = RemoteEntity::of(entity, dimension, wanted);
        entries.uuid_by_id.insert(entry.id, entry.uuid);
        entries.by_uuid.insert(entry.uuid, entry);
        entries.want_owner_of(entity, dimension);
    }

    /// `entity` is back in its level: bring its copy up to date (nothing when it is not published there).
    pub(super) fn refresh(&self, entity: &dyn Entity, dimension: &DimensionKey) {
        let mut entries = self.0.lock();
        let wanted = entries.is_wanted(&entity.uuid());
        if let Some(entry) = entries.by_uuid.get_mut(&entity.uuid())
            && entry.id == entity.id()
            && entry.dimension == *dimension
        {
            entry.update(entity, wanted);
            entries.want_owner_of(entity, dimension);
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
                entries.wanted_until.remove(&uuid);
            }
            Some((true, false)) => {}
            _ => {
                entries.uuid_by_id.remove(&id);
            }
        }
    }

    /// Keeps the player `uuid` wanted: its copy carries the full saved state from the next refresh on.
    pub(super) fn want(&self, uuid: Uuid) {
        self.0.lock().want(uuid);
    }

    /// One level tick passed: players nobody asked for lately stop being wanted.
    pub(super) fn advance_clock(&self) {
        let mut entries = self.0.lock();
        entries.clock += 1;
        let clock = entries.clock;
        entries.wanted_until.retain(|_, until| *until >= clock);
    }

    /// `f` on the copy of the player `uuid` while it is wanted (what a stand-in is kept for).
    pub(super) fn with_wanted<R>(
        &self,
        uuid: Uuid,
        f: impl FnOnce(&RemoteEntity) -> R,
    ) -> Option<R> {
        let entries = self.0.lock();
        entries
            .is_wanted(&uuid)
            .then(|| entries.by_uuid.get(&uuid).map(f))
            .flatten()
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

    fn player(level: &mut ServerLevel, n: u128) -> (Uuid, EntityId) {
        level.entity_manager.update_chunk_status(
            mc_core::pos::chunk_pos::ChunkPos::new(0, 0),
            crate::world::level::entity::Visibility::Ticking,
        );
        let (uuid, id) = (Uuid::from_u128(n), n as EntityId);
        let mut player = ServerPlayer::new(
            id,
            level.dimension.clone(),
            mc_protocol::codec::GameProfile {
                id: uuid,
                name: format!("P{n}"),
                properties: vec![],
            },
            Default::default(),
        );
        // a player of a few hours knows a few hundred recipes
        for recipe in 0..400 {
            player
                .recipe_book
                .add(Identifier::parse(&format!("minecraft:recipe_{recipe}")).unwrap());
        }
        level.add_player(Box::new(player));
        (uuid, id)
    }

    /// One tick step of a player of `level`: out of its slot, ticked, back.
    fn tick_player(level: &mut ServerLevel, id: EntityId) {
        let mut player = level.take_entity(id).expect("player");
        player.base_mut().tick_count += 1;
        level.put_back_entity(id, player);
    }

    /// The saved data of a player is only copied while a level reads it through a stand-in or an entity it owns exists.
    #[test]
    fn player_data_is_copied_only_for_wanted_players() {
        let directory = EntityDirectory::default();
        let mut overworld = level("minecraft:overworld", &directory);
        let nether = level("minecraft:the_nether", &directory);
        let (uuid, id) = player(&mut overworld, 7);
        tick_player(&mut overworld, id);
        assert!(directory.get(uuid).unwrap().player.is_none());

        // a level asks for the stand-in: the copy carries the data from the next put-back on, and the stand-in loads it
        let stand_in = nether.get_entity_or_remote_player(uuid).expect("stand-in");
        assert_eq!(stand_in.id(), id);
        tick_player(&mut overworld, id);
        assert!(directory.get(uuid).unwrap().player.is_some());

        // nobody asks any more: the copy drops the data and the level drops the stand-in
        for _ in 0..=WANT_WINDOW {
            directory.advance_clock();
        }
        tick_player(&mut overworld, id);
        assert!(directory.get(uuid).unwrap().player.is_none());
        let mut nether = nether;
        nether.remote_targets.refresh(&directory, &nether.dimension);
        assert!(!nether.remote_targets.contains(id));
    }

    /// A change made between two ticks of a wanted player (a packet, a hit, a command) reaches its copy at the next put-back.
    #[test]
    fn a_wanted_player_is_copied_again_within_one_tick() {
        let directory = EntityDirectory::default();
        let mut overworld = level("minecraft:overworld", &directory);
        let nether = level("minecraft:the_nether", &directory);
        let (uuid, id) = player(&mut overworld, 9);
        nether.get_entity_or_remote_player(uuid).expect("stand-in");
        tick_player(&mut overworld, id);
        let tags_of = |directory: &EntityDirectory| {
            let remote = directory.get(uuid).unwrap();
            remote.player.unwrap().saved.get("Tags").cloned()
        };
        assert!(tags_of(&directory).is_none());

        let mut player = overworld.take_entity(id).expect("player");
        player.base_mut().add_tag("changed");
        overworld.put_back_entity(id, player);
        assert!(tags_of(&directory).is_some());
    }

    /// A projectile that names a player as its owner keeps the player's data copied while it exists.
    #[test]
    fn an_owned_projectile_keeps_its_owner_wanted() {
        let directory = EntityDirectory::default();
        let mut overworld = level("minecraft:overworld", &directory);
        let mut nether = level("minecraft:the_nether", &directory);
        let (uuid, id) = player(&mut overworld, 8);
        let mut arrow = factory::create(
            EntityType::ARROW,
            &nether,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Command,
                ignore_checks: true,
            },
        )
        .expect("arrow");
        arrow
            .as_projectile_mut()
            .unwrap()
            .set_owner_uuid(Some(uuid));
        nether.entity_manager.update_chunk_status(
            mc_core::pos::chunk_pos::ChunkPos::new(0, 0),
            crate::world::level::entity::Visibility::Ticking,
        );
        assert!(nether.add_fresh_entity(arrow));
        tick_player(&mut overworld, id);
        assert!(directory.get(uuid).unwrap().player.is_some());
    }

    /// Cost of the put-backs of 20 players in 3 levels over 200 ticks, with no stand-in anywhere:
    /// `cargo test -p mc-game-core player_copy_cost -- --ignored --nocapture`.
    #[test]
    #[ignore = "timing probe"]
    fn player_copy_cost() {
        let directory = EntityDirectory::default();
        let names = [
            "minecraft:overworld",
            "minecraft:the_nether",
            "minecraft:the_end",
        ];
        let mut levels: Vec<ServerLevel> = names.iter().map(|n| level(n, &directory)).collect();
        let ids: Vec<(usize, EntityId)> = (0..20u128)
            .map(|n| {
                let l = n as usize % 3;
                (l, player(&mut levels[l], n + 1).1)
            })
            .collect();
        let start = std::time::Instant::now();
        for _ in 0..200 {
            for &(l, id) in &ids {
                tick_player(&mut levels[l], id);
            }
        }
        let elapsed = start.elapsed();
        eprintln!(
            "{:?} per server tick for 20 players in 3 levels",
            elapsed / 200
        );
    }
}
