//! `net.minecraft.world.entity.monster.warden.Warden`.
//!
//! Client-only parts of the class (`AnimationState`s, tendril/heart animation, digging particles, the heartbeat sound,
//! `handleEntityEvent`) are not ported: the server only broadcasts the entity events (4, 61, 62).

use super::anger_level::AngerLevel;
use super::anger_management::AngerManagement;
use super::warden_ai;
use crate::define_accessor;
use crate::registry::RegistryAccess;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::ai::brain::{Brain, Provider};
use crate::world::entity::ai::memory::memory_module_type::{
    ATTACK_TARGET, CANT_REACH_WALK_TARGET_SINCE, DIG_COOLDOWN, IS_EMERGING, NEAREST_VISIBLE_NEMESIS, RECENT_PROJECTILE, ROAR_TARGET, TOUCH_COOLDOWN,
    VIBRATION_COOLDOWN,
};
use crate::world::entity::ai::memory::{BrainPacked, EntityRef, Unit};
use crate::world::entity::ai::navigation::{NavClass, NavigationData, PathNavigation};
use crate::world::entity::ai::sensing::sensor_type::{NEAREST_PLAYERS, WARDEN_ENTITY_SENSOR};
use crate::world::entity::ai::behavior::warden::sonic_boom::SonicBoom;
use crate::world::entity::dimensions::EntityDimensions;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::living_entity::damage;
use crate::world::entity::mob::{self, MobData, MobKind};
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::gameevent::vibrations::{self, VibrationData, VibrationSystem};
use crate::world::level::gameevent::{DeliveryMode, DynamicGameEventListener, DynamicListenerAction, GameEventContext, GameEventListener, ListenerOwner, PositionSource, SourceEntityInfo};
use crate::world::level::pathfinder::PathType;
use crate::world::level::pathfinder::node_evaluator::{EvaluatorState, NodeEvaluator};
use crate::world::level::pathfinder::PathFinder;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{EntityType, GameEvent, SoundEvent};
use mc_nbt::{CompoundTag, Tag};

define_accessor!(pub CLIENT_ANGER_LEVEL: Int = Warden);

/// `VIBRATION_COOLDOWN_TICKS`.
const VIBRATION_COOLDOWN_TICKS: i64 = 40;
const DARKNESS_RADIUS: i32 = 20;
const DARKNESS_INTERVAL: i32 = 120;
const ANGERMANAGEMENT_TICK_DELAY: i32 = 20;
const PROJECTILE_ANGER: i32 = 10;
const ON_HURT_ANGER_BOOST: i32 = 20;
const RECENT_PROJECTILE_TICK_THRESHOLD: i64 = 100;
const TOUCH_COOLDOWN_TICKS: i64 = 20;
const PROJECTILE_ANGER_DISTANCE: f64 = 30.0;
/// `GAME_EVENT_LISTENER_RANGE` of `Warden.VibrationUser`.
const GAME_EVENT_LISTENER_RANGE: i32 = 16;

/// `BRAIN_PROVIDER`.
static BRAIN_PROVIDER: Provider = Provider::with_memories(
    &[NEAREST_VISIBLE_NEMESIS.id(), RECENT_PROJECTILE.id(), TOUCH_COOLDOWN.id(), VIBRATION_COOLDOWN.id()],
    &[&NEAREST_PLAYERS, &WARDEN_ENTITY_SENSOR],
    warden_ai::get_activities,
);

/// `Entity.closerThan(entity, distanceXZ, distanceY)`.
pub fn closer_than_xz_y(a: &dyn Entity, b: &dyn Entity, distance_xz: f64, distance_y: f64) -> bool {
    let (pa, pb) = (a.position(), b.position());
    let (dx, dy, dz) = (pb.x - pa.x, pb.y - pa.y, pb.z - pa.z);
    dx * dx + dz * dz < distance_xz * distance_xz && dy * dy < distance_y * distance_y
}

/// `Warden`.
pub struct Warden {
    pub mob: MobData,
    anger_management: AngerManagement,
    vibration_data: VibrationData,
    dynamic_game_event_listener: DynamicGameEventListener,
    /// `EntityPositionSource(Warden.this, Warden.this.getEyeHeight())`, evaluated in the constructor.
    position_source_y_offset: f32,
}

impl Warden {
    /// `new Warden(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Monster, |b| {
            b.define(CLIENT_ANGER_LEVEL, 0);
        });
        let eye_height = mob.living.entity.eye_height;
        let mut warden = Warden {
            mob,
            anger_management: AngerManagement::default(),
            vibration_data: VibrationData::default(),
            dynamic_game_event_listener: DynamicGameEventListener::new(),
            position_source_y_offset: eye_height,
        };
        // `LivingEntity.<init>`: `this.brain = this.makeBrain(Brain.Packed.EMPTY)`
        let brain = warden.make_brain(&BrainPacked::EMPTY);
        warden.mob.living.brain = brain;
        warden.mob.xp_reward = 5;
        let navigation = WardenNavigation::new(&warden);
        warden.mob.set_navigation(Box::new(navigation));
        if let Some(navigation) = warden.mob.navigation.as_deref_mut() {
            navigation.set_can_float(true);
        }
        warden.set_pathfinding_malus(PathType::UNPASSABLE_RAIL, 0.0);
        warden.set_pathfinding_malus(PathType::DAMAGING, 8.0);
        warden.set_pathfinding_malus(PathType::POWDER_SNOW, 8.0);
        warden.set_pathfinding_malus(PathType::LAVA, 8.0);
        warden.set_pathfinding_malus(PathType::FIRE, 0.0);
        warden.set_pathfinding_malus(PathType::FIRE_IN_NEIGHBOR, 0.0);
        warden
    }

    /// `isDiggingOrEmerging()`.
    pub fn is_digging_or_emerging(&self) -> bool {
        matches!(self.base().get_pose(), Pose::Digging | Pose::Emerging)
    }

    /// `hasPose(pose)`.
    pub fn has_pose(&self, pose: Pose) -> bool {
        self.base().get_pose() == pose
    }

    /// `getClientAngerLevel()`.
    pub fn get_client_anger_level(&self) -> i32 {
        self.base().entity_data.get(CLIENT_ANGER_LEVEL)
    }

    /// `syncClientAngerLevel()`.
    fn sync_client_anger_level(&mut self, level: &ServerLevel) {
        let anger = self.get_active_anger(level);
        self.base_mut().entity_data.set(CLIENT_ANGER_LEVEL, anger);
    }

    /// `getTarget()` of the warden: the brain's `ATTACK_TARGET` through `asValidTarget`.
    pub fn get_target_valid(&self, level: &ServerLevel) -> Option<EntityId> {
        mob::tick::as_valid_target(self, level, self.attack_target_raw())
    }

    fn attack_target_raw(&self) -> Option<EntityId> {
        self.brain().get_memory(&ATTACK_TARGET).map(|r| r.0)
    }

    /// `canTargetEntity(entity)` (`@Contract("null->false")`).
    pub fn can_target_entity_ref(&self, level: &ServerLevel, entity: &dyn Entity) -> bool {
        entity.living().is_some()
            && crate::world::entity::ai::sensing::mob_sensors::no_creative_or_spectator(entity)
            && !mob::ext::is_allied_to(level, self, entity)
            && entity.get_type() != EntityType::ARMOR_STAND
            && entity.get_type() != EntityType::WARDEN
            && !entity.is_invulnerable()
            && !entity.is_dead_or_dying()
            && level.world_border().is_within_bounds_aabb(&entity.get_bounding_box())
    }

    /// `canTargetEntity(entity)` for an id.
    pub fn can_target_id(&self, level: &ServerLevel, entity: Option<EntityId>) -> bool {
        entity.and_then(|id| level.get_entity(id)).is_some_and(|e| self.can_target_entity_ref(level, e))
    }

    /// `applyDarknessAround(level, position, source, darknessRadius)`.
    pub fn apply_darkness_around(level: &mut ServerLevel, position: Vec3, source: Option<&dyn Entity>, darkness_radius: i32) {
        crate::world::block_entity::sculk_shrieker_block_entity::apply_darkness_around(level, position, source, darkness_radius);
    }

    /// `playListeningSound()`.
    fn play_listening_sound(&mut self, level: &mut ServerLevel) {
        if !self.has_pose(Pose::Roaring) {
            let pitch = self.get_voice_pitch();
            let sound = self.get_anger_level(level).get_listening_sound();
            self.play_sound(level, sound, 10.0, pitch);
        }
    }

    /// `getAngerLevel()`.
    pub fn get_anger_level(&self, level: &ServerLevel) -> AngerLevel {
        AngerLevel::by_anger(self.get_active_anger(level))
    }

    /// `getAngerLevel()` where `getTarget()` is the unchecked brain target (callers without a level: sounds).
    fn get_anger_level_unchecked(&self) -> AngerLevel {
        AngerLevel::by_anger(self.anger_management.get_active_anger(self.attack_target_raw()))
    }

    /// `getActiveAnger()`.
    fn get_active_anger(&self, level: &ServerLevel) -> i32 {
        self.anger_management.get_active_anger(self.get_target_valid(level))
    }

    /// `clearAnger(entity)`.
    pub fn clear_anger(&mut self, entity: EntityId) {
        self.anger_management.clear_anger(entity);
    }

    /// `increaseAngerAt(entity)`.
    pub fn increase_anger_at_default(&mut self, level: &mut ServerLevel, entity: Option<EntityId>) {
        self.increase_anger_at(level, entity, 35, true);
    }

    /// `increaseAngerAt(entity, amount, playSound)`.
    pub fn increase_anger_at(&mut self, level: &mut ServerLevel, entity: Option<EntityId>, amount: i32, play_sound: bool) {
        if !self.mob.is_no_ai() && self.can_target_id(level, entity) {
            let Some(entity_id) = entity else { return };
            warden_ai::set_dig_cooldown(self);
            let maybe_switch_target = !self.get_target_valid(level).and_then(|t| level.get_entity(t)).is_some_and(|t| t.is_player());
            let (new_anger, is_player) = {
                let target = level.get_entity(entity_id).expect("checked by can_target_id");
                (self.anger_management.increase_anger(target, amount), target.is_player())
            };
            if is_player && maybe_switch_target && AngerLevel::by_anger(new_anger).is_angry() {
                self.brain_mut().erase_memory(&ATTACK_TARGET);
            }
            if play_sound {
                self.play_listening_sound(level);
            }
        }
    }

    /// `getEntityAngryAt()`.
    pub fn get_entity_angry_at(&self, level: &ServerLevel) -> Option<EntityId> {
        if self.get_anger_level(level).is_angry() { self.anger_management.get_active_entity(level, &|e| self.can_target_entity_ref(level, e)) } else { None }
    }

    /// `setAttackTarget(target)`.
    pub fn set_attack_target(&mut self, target: EntityId) {
        self.brain_mut().erase_memory(&ROAR_TARGET);
        self.brain_mut().set_memory(&ATTACK_TARGET, EntityRef(target));
        self.brain_mut().erase_memory(&CANT_REACH_WALK_TARGET_SINCE);
        SonicBoom::set_cooldown(self, 200);
    }

    /// `getAngerManagement()`.
    pub fn get_anger_management(&self) -> &AngerManagement {
        &self.anger_management
    }

    /// `getVibrationData()`.
    pub fn get_vibration_data(&self) -> &VibrationData {
        &self.vibration_data
    }

    fn vibration_position_source(&self) -> PositionSource {
        PositionSource::Entity { uuid: self.uuid(), y_offset: self.position_source_y_offset }
    }

    /// `Warden.VibrationUser.getPositionSource()`'s current position (`source.getPosition(level)`).
    fn listener_position(&self) -> Vec3 {
        self.position().add_xyz(0.0, f64::from(self.position_source_y_offset), 0.0)
    }
}

/// The anonymous `GroundPathNavigation` of `Warden.createNavigation`: a walk evaluator and a `PathFinder` whose distance
/// is the horizontal one.
struct WardenEvaluator(EvaluatorState);

impl NodeEvaluator for WardenEvaluator {
    fn state(&self) -> &EvaluatorState {
        &self.0
    }
    fn state_mut(&mut self) -> &mut EvaluatorState {
        &mut self.0
    }
}

/// `Warden.createNavigation(level)`.
pub struct WardenNavigation {
    data: NavigationData,
}

impl WardenNavigation {
    fn new(warden: &Warden) -> Self {
        let data = NavigationData::with_path_finder(warden, NavClass::Ground, |max_visited_nodes| {
            PathFinder::new(Box::new(WardenEvaluator(EvaluatorState::walk())), max_visited_nodes).with_distance(|from, to| from.distance_to_xz(to))
        });
        WardenNavigation { data }
    }
}

impl PathNavigation for WardenNavigation {
    fn data(&self) -> &NavigationData {
        &self.data
    }
    fn data_mut(&mut self) -> &mut NavigationData {
        &mut self.data
    }
}

impl Entity for Warden {
    crate::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:warden")
    }

    fn make_brain(&mut self, packed: &BrainPacked) -> Brain {
        BRAIN_PROVIDER.make_brain(self, packed)
    }

    fn get_add_entity_packet(&self, server_entity: &crate::server::level::ServerEntity) -> mc_protocol::packets::play::ClientboundAddEntityPacket {
        server_entity.add_entity_packet(self.id(), self.uuid(), self.get_type(), i32::from(self.has_pose(Pose::Emerging)))
    }

    fn check_spawn_obstruction(&self, level: &dyn ServerLevelAccessor) -> bool {
        mob::spawn::check_spawn_obstruction(self, level) && level.no_collision_aabb(&self.get_type().dimensions().make_bounding_box_vec(self.position()))
    }

    fn get_walk_target_value(&self, _pos: BlockPos, _level: &dyn crate::world::level::LevelReader) -> f32 {
        0.0
    }

    fn is_invulnerable_to(&mut self, level: &mut ServerLevel, source: &DamageSource) -> bool {
        if self.is_digging_or_emerging() && !source.is_tag(level, crate::world::damagesource::DamageTypeTags::BYPASSES_INVULNERABILITY) { true } else { damage::is_invulnerable_to(self, level, source) }
    }

    fn can_ride(&self, _vehicle: &dyn Entity) -> bool {
        false
    }

    fn get_seconds_to_disable_blocking(&self) -> f32 {
        5.0
    }

    fn next_step(&self) -> f32 {
        self.base().move_dist + 0.55
    }

    fn dampens_vibrations(&self) -> bool {
        true
    }

    fn get_sound_volume(&self) -> f32 {
        4.0
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        if !self.has_pose(Pose::Roaring) && !self.is_digging_or_emerging() { Some(self.get_anger_level_unchecked().get_ambient_sound()) } else { None }
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_WARDEN_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_WARDEN_DEATH)
    }

    fn play_step_sound(&mut self, level: &mut ServerLevel, _pos: BlockPos, _state: mc_data::block::BlockState) {
        self.play_sound(level, SoundEvent::ENTITY_WARDEN_STEP, 10.0, 1.0);
    }

    fn do_hurt_target(&mut self, level: &mut ServerLevel, target: &mut dyn Entity) -> bool {
        level.broadcast_entity_event(self.id(), 4);
        let pitch = self.get_voice_pitch();
        self.play_sound(level, SoundEvent::ENTITY_WARDEN_ATTACK_IMPACT, 10.0, pitch);
        SonicBoom::set_cooldown(self, 40);
        mob::combat::mob_do_hurt_target(self, level, target)
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        vibrations::tick(level, self);
        if self.is_persistence_required() || self.requires_custom_persistence() {
            warden_ai::set_dig_cooldown(self);
        }
        mob::tick::tick(self, level);
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        Brain::tick(level, self);
        let tick_count = self.base().tick_count;
        if tick_count.wrapping_add(self.id()) % DARKNESS_INTERVAL == 0 {
            let position = self.position();
            Warden::apply_darkness_around(level, position, Some(self.as_dyn()), DARKNESS_RADIUS);
        }
        if tick_count % ANGERMANAGEMENT_TICK_DELAY == 0 {
            let mut management = std::mem::take(&mut self.anger_management);
            management.tick(level, &|e| self.can_target_entity_ref(level, e));
            self.anger_management = management;
            self.sync_client_anger_level(level);
        }
        warden_ai::update_activity(self.brain_mut());
    }

    fn ignore_explosion(&self, _explosion: &dyn crate::world::level::explosion::Explosion) -> bool {
        self.is_digging_or_emerging()
    }

    fn update_dynamic_game_event_listener(&mut self, level: &mut ServerLevel, action: DynamicListenerAction) {
        let owner = ListenerOwner::Entity(self.id());
        let source = self.vibration_position_source();
        let position = self.listener_position();
        self.dynamic_game_event_listener.apply(action, level, owner, source, GAME_EVENT_LISTENER_RANGE, DeliveryMode::Unspecified, position);
    }

    fn game_event_listener(&mut self) -> Option<&mut dyn GameEventListener> {
        Some(self)
    }

    fn can_attack(&self, level: &ServerLevel, target: &dyn Entity) -> bool {
        self.can_target_entity_ref(level, target)
    }

    fn can_target_entity(&self, level: &ServerLevel, entity: &dyn Entity) -> bool {
        self.can_target_entity_ref(level, entity)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put("anger", Tag::Compound(self.anger_management.to_nbt()));
        if let Ok(listener) = mc_dfu::encode_nbt(VibrationData::codec(), &self.vibration_data).into_result() {
            out.put("listener", listener);
        }
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.anger_management = input.get_compound("anger").map(AngerManagement::from_nbt).unwrap_or_default();
        // `syncClientAngerLevel()`: no target is known yet (the brain's attack target is not persisted) -> highest anger
        let anger = self.anger_management.get_active_anger(None);
        self.base_mut().entity_data.set(CLIENT_ANGER_LEVEL, anger);
        self.vibration_data = crate::world::block_entity::read_codec(input, "listener", VibrationData::codec()).unwrap_or_default();
    }

    fn get_target(&self) -> Option<EntityId> {
        self.attack_target_raw()
    }

    fn remove_when_far_away(&self, _distance_to_closest_player: f64) -> bool {
        false
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        self.brain_mut().set_memory_with_expiry(&DIG_COOLDOWN, Unit, i64::from(warden_ai::DIGGING_COOLDOWN));
        if reason == EntitySpawnReason::Triggered {
            self.set_pose(Pose::Emerging);
            self.brain_mut().set_memory_with_expiry(&IS_EMERGING, Unit, i64::from(warden_ai::EMERGE_DURATION));
            if let Some(server_level) = level.as_server_level_mut() {
                self.play_sound(server_level, SoundEvent::ENTITY_WARDEN_AGITATED, 5.0, 1.0);
            }
        }
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage_amount: f32) -> bool {
        let was_hurt = damage::hurt_server(self, level, source, damage_amount);
        if !self.mob.is_no_ai() && !self.is_digging_or_emerging() {
            let attacker = source.get_entity();
            self.increase_anger_at(level, attacker, AngerLevel::Angry.get_minimum_anger() + ON_HURT_ANGER_BOOST, false);
            if self.brain().get_memory(&ATTACK_TARGET).is_none()
                && let Some(attacker_id) = attacker
                && let Some(living_attacker) = level.get_entity(attacker_id).filter(|e| e.living().is_some())
                && (source.is_direct() || crate::world::entity::ai::sensing::closer_than(self, living_attacker, 5.0))
            {
                self.set_attack_target(attacker_id);
            }
        }
        was_hurt
    }

    fn get_dimensions(&self, pose: Pose) -> EntityDimensions {
        let dimensions = EntityDimensions::of_type(self.get_type());
        let _ = pose;
        if self.is_digging_or_emerging() { EntityDimensions::fixed(dimensions.width, 1.0) } else { dimensions }
    }

    fn is_pushable(&self, level: &ServerLevel) -> bool {
        !self.is_digging_or_emerging() && crate::world::entity::living_entity::push::living_is_pushable(self, level)
    }

    fn do_push(&mut self, level: &mut ServerLevel, other: &mut dyn Entity) {
        if !self.mob.is_no_ai() && !self.brain().has_memory_value(TOUCH_COOLDOWN) {
            self.brain_mut().set_memory_with_expiry(&TOUCH_COOLDOWN, Unit, TOUCH_COOLDOWN_TICKS);
            self.increase_anger_at_default(level, Some(other.id()));
            warden_ai::set_disturbance_location(level, self, other.block_position());
        }
        crate::world::entity::living_entity::push::do_push(level, self, other);
    }
}

impl GameEventListener for Warden {
    fn get_listener_source(&self) -> PositionSource {
        self.vibration_position_source()
    }

    fn get_listener_radius(&self) -> i32 {
        GAME_EVENT_LISTENER_RANGE
    }

    fn handle_game_event(&mut self, level: &mut ServerLevel, event: GameEvent, context: &GameEventContext, source_position: Vec3) -> bool {
        vibrations::handle_game_event(level, self, event, context, source_position)
    }
}

impl VibrationSystem for Warden {
    fn with_vibration_data<R>(&mut self, _level: &mut ServerLevel, f: impl FnOnce(&mut VibrationData) -> R) -> R {
        f(&mut self.vibration_data)
    }

    fn get_listener_radius(&self) -> i32 {
        GAME_EVENT_LISTENER_RANGE
    }

    fn get_position_source(&self) -> PositionSource {
        self.vibration_position_source()
    }

    fn get_listenable_events(&self) -> &'static str {
        "warden_can_listen"
    }

    fn can_trigger_avoid_vibration(&self) -> bool {
        true
    }

    fn can_receive_vibration(&mut self, level: &mut ServerLevel, pos: BlockPos, _event: GameEvent, context: &GameEventContext) -> bool {
        if !self.mob.is_no_ai()
            && !self.is_dead_or_dying()
            && !self.brain().has_memory_value(VIBRATION_COOLDOWN)
            && !self.is_digging_or_emerging()
            && level.world_border().is_within_bounds_block(&pos)
        {
            let living_source = context.source_entity.and_then(|id| level.get_entity(id)).filter(|e| e.living().is_some());
            !living_source.is_some_and(|e| !self.can_target_entity_ref(level, e))
        } else {
            false
        }
    }

    fn on_receive_vibration(
        &mut self,
        level: &mut ServerLevel,
        pos: BlockPos,
        _event: GameEvent,
        source_entity: Option<SourceEntityInfo>,
        projectile_owner: Option<SourceEntityInfo>,
        _receiving_distance: f32,
    ) {
        if self.is_dead_or_dying() {
            return;
        }
        self.brain_mut().set_memory_with_expiry(&VIBRATION_COOLDOWN, Unit, VIBRATION_COOLDOWN_TICKS);
        level.broadcast_entity_event(self.id(), 61);
        let pitch = self.get_voice_pitch();
        self.play_sound(level, SoundEvent::ENTITY_WARDEN_TENDRIL_CLICKS, 5.0, pitch);
        let mut suspicious_pos = pos;
        let projectile_owner_id = projectile_owner.map(|o| o.id);
        if let Some(owner) = projectile_owner_id {
            if level.get_entity(owner).is_some_and(|e| crate::world::entity::ai::sensing::closer_than(self, e, PROJECTILE_ANGER_DISTANCE)) {
                if self.brain().has_memory_value(RECENT_PROJECTILE) {
                    if self.can_target_id(level, Some(owner)) {
                        suspicious_pos = level.get_entity(owner).expect("checked by can_target_id").block_position();
                    }
                    self.increase_anger_at_default(level, Some(owner));
                } else {
                    self.increase_anger_at(level, Some(owner), PROJECTILE_ANGER, true);
                }
            }
            self.brain_mut().set_memory_with_expiry(&RECENT_PROJECTILE, Unit, RECENT_PROJECTILE_TICK_THRESHOLD);
        } else {
            self.increase_anger_at_default(level, source_entity.map(|s| s.id));
        }
        if !self.get_anger_level(level).is_angry() {
            let active_entity = self.anger_management.get_active_entity(level, &|e| self.can_target_entity_ref(level, e));
            if projectile_owner_id.is_some() || active_entity.is_none() || active_entity == source_entity.map(|s| s.id) {
                warden_ai::set_disturbance_location(level, self, suspicious_pos);
            }
        }
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static WARDEN: EntityRegistration = EntityRegistration { entity_type: EntityType::WARDEN, create: |ty, id, registries| Box::new(Warden::new(ty, id, registries)) };
