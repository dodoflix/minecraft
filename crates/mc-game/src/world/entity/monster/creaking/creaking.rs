//! `net.minecraft.world.entity.monster.creaking.Creaking`.
//!
//! Client-only parts of the class (animation states, the eye blink, `handleEntityEvent`, `updateWalkAnimation`) are not
//! ported; the server only broadcasts the entity events (4, 66).

use super::creaking_ai;
use crate::define_accessor;
use crate::registry::RegistryAccess;
use crate::server::level::ServerLevel;
use crate::world::block_entity::CreakingHeartBlockEntity;
use crate::world::damagesource::{DamageSource, DamageTypeTags};
use crate::world::entity::ai::brain::{Brain, Provider};
use crate::world::entity::ai::control::body_rotation_control::{BodyRotationControlBase, base_client_tick};
use crate::world::entity::ai::control::jump_control::{self, JumpControl, JumpControlBase};
use crate::world::entity::ai::control::look_control::{self, LookControl, LookControlBase};
use crate::world::entity::ai::control::move_control::{self, MoveControl, MoveControlBase};
use crate::world::entity::ai::control::BodyRotationControl;
use crate::world::entity::ai::memory::memory_module_type::{ATTACK_TARGET, NEAREST_PLAYERS};
use crate::world::entity::ai::memory::{BrainPacked, EntityRef};
use crate::world::entity::ai::navigation::{NavClass, NavigationData, PathNavigation};
use crate::world::entity::ai::sensing::sensor_type::{NEAREST_LIVING_ENTITIES, NEAREST_PLAYERS as NEAREST_PLAYERS_SENSOR};
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::living_entity::damage;
use crate::world::entity::mob::{self, MobData, MobKind};
use crate::world::entity::{Entity, EntityId, RemovalReason};
use crate::world::level::pathfinder::node_evaluator::{EvaluatorState, NodeEvaluator, PathEnv, default_get_path_type};
use crate::world::level::pathfinder::{PathFinder, PathType};
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_data::enums::CreakingHeartState;
use mc_data::{Attribute, EntityType, GameEvent, ParticleType, SoundEvent};
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::packets::play::ParticleOptions;

define_accessor!(pub CAN_MOVE: Boolean = Creaking);
define_accessor!(pub IS_ACTIVE: Boolean = Creaking);
define_accessor!(pub IS_TEARING_DOWN: Boolean = Creaking);
define_accessor!(pub HOME_POS: OptionalBlockPos = Creaking);

const ATTACK_ANIMATION_DURATION: i32 = 15;
/// `ACTIVATION_RANGE_SQ`.
const ACTIVATION_RANGE_SQ: f64 = 144.0;
/// `ATTACK_INTERVAL`.
pub const ATTACK_INTERVAL: i32 = 40;
/// `SPEED_MULTIPLIER_WHEN_IDLING`.
pub const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 0.3;
const INVULNERABILITY_ANIMATION_DURATION: i32 = 8;
/// `TWITCH_DEATH_DURATION`.
pub const TWITCH_DEATH_DURATION: i32 = 45;
const MAX_PLAYER_STUCK_COUNTER: i32 = 4;
/// `HomeNodeEvaluator.MAX_DISTANCE_TO_HOME_SQ`.
const MAX_DISTANCE_TO_HOME_SQ: f64 = 1024.0;

/// `BRAIN_PROVIDER`.
static BRAIN_PROVIDER: Provider = Provider::with_activities(&[&NEAREST_LIVING_ENTITIES, &NEAREST_PLAYERS_SENSOR], creaking_ai::get_activities);

/// `Creaking`.
pub struct Creaking {
    pub mob: MobData,
    attack_animation_remaining_ticks: i32,
    invulnerability_animation_remaining_ticks: i32,
    player_stuck_counter: i32,
}

/// `creaking.canMove()` of a mob known to be a creaking (the controls and behaviors only run on creakings).
pub fn can_move(mob: &dyn Entity) -> bool {
    mob.base().entity_data.get(CAN_MOVE)
}

impl Creaking {
    /// `new Creaking(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Monster, |b| {
            b.define(CAN_MOVE, true);
            b.define(IS_ACTIVE, false);
            b.define(IS_TEARING_DOWN, false);
            b.define(HOME_POS, None);
        });
        let mut creaking = Creaking { mob, attack_animation_remaining_ticks: 0, invulnerability_animation_remaining_ticks: 0, player_stuck_counter: 0 };
        // `LivingEntity.<init>`: `this.brain = this.makeBrain(Brain.Packed.EMPTY)`
        let brain = creaking.make_brain(&BrainPacked::EMPTY);
        creaking.mob.living.brain = brain;
        // `Mob.<init>`: `createBodyControl()`
        creaking.mob.body_rotation_control = Some(Box::new(CreakingBodyRotationControl::default()));
        creaking.mob.look_control = Some(Box::new(CreakingLookControl::default()));
        creaking.mob.move_control = Some(Box::new(CreakingMoveControl::default()));
        creaking.mob.jump_control = Some(Box::new(CreakingJumpControl::default()));
        let navigation = CreakingNavigation::new(&creaking);
        creaking.mob.set_navigation(Box::new(navigation));
        if let Some(navigation) = creaking.mob.navigation.as_deref_mut() {
            navigation.set_can_float(true);
        }
        creaking.mob.xp_reward = 0;
        creaking
    }

    /// `setTransient(pos)`.
    pub fn set_transient(&mut self, pos: BlockPos) {
        self.set_home_pos(pos);
        self.set_pathfinding_malus(PathType::DAMAGING, 8.0);
        self.set_pathfinding_malus(PathType::POWDER_SNOW, 8.0);
        self.set_pathfinding_malus(PathType::LAVA, 8.0);
        self.set_pathfinding_malus(PathType::FIRE, 0.0);
        self.set_pathfinding_malus(PathType::FIRE_IN_NEIGHBOR, 0.0);
    }

    /// `isHeartBound()`.
    pub fn is_heart_bound(&self) -> bool {
        self.get_home_pos().is_some()
    }

    /// `canMove()`.
    pub fn can_move(&self) -> bool {
        can_move(self)
    }

    /// `setHomePos(pos)`.
    pub fn set_home_pos(&mut self, pos: BlockPos) {
        self.base_mut().entity_data.set(HOME_POS, Some(pos));
    }

    /// `getHomePos()`.
    pub fn get_home_pos(&self) -> Option<BlockPos> {
        self.base().entity_data.get(HOME_POS)
    }

    /// `setTearingDown()`.
    pub fn set_tearing_down(&mut self) {
        self.base_mut().entity_data.set(IS_TEARING_DOWN, true);
    }

    /// `isTearingDown()`.
    pub fn is_tearing_down(&self) -> bool {
        self.base().entity_data.get(IS_TEARING_DOWN)
    }

    /// `setIsActive(active)`.
    pub fn set_is_active(&mut self, active: bool) {
        self.base_mut().entity_data.set(IS_ACTIVE, active);
    }

    /// `isActive()`.
    pub fn is_active(&self) -> bool {
        self.base().entity_data.get(IS_ACTIVE)
    }

    /// `blameSourceForDamage(source)`: the player responsible for the damage (`lastHurtByPlayer`).
    pub fn blame_source_for_damage(&mut self, level: &ServerLevel, source: &DamageSource) -> Option<EntityId> {
        damage::resolve_mob_responsible_for_damage(self, level, source);
        damage::resolve_player_responsible_for_damage(self, source);
        let uuid = self.mob.living.last_hurt_by_player?;
        level.get_entity_by_uuid(uuid).filter(|e| e.is_player()).map(|e| e.id())
    }

    /// `this.level().getBlockEntity(homePos) instanceof CreakingHeartBlockEntity heart && heart.isProtector(this)`.
    fn has_protection_from_creaking_heart(&self, level: &mut ServerLevel, home_pos: BlockPos) -> bool {
        let id = self.id();
        level
            .with_block_entity(home_pos, |level, be| be.as_any_mut().downcast_mut::<CreakingHeartBlockEntity>().is_some_and(|heart| heart.is_protector(level, id)))
            .unwrap_or(false)
    }

    /// `tearDown()`.
    pub fn tear_down(&mut self, level: &mut ServerLevel) {
        let bounding_box = self.get_bounding_box();
        let center = bounding_box.get_center();
        let x_spread = bounding_box.get_xsize() * 0.3;
        let y_spread = bounding_box.get_ysize() * 0.3;
        let z_spread = bounding_box.get_zsize() * 0.3;
        level.send_particles(
            ParticleOptions::Block { particle_type: ParticleType::BLOCK_CRUMBLE, state: Block::PALE_OAK_WOOD.default_state() },
            center.x,
            center.y,
            center.z,
            100,
            x_spread,
            y_spread,
            z_spread,
            0.0,
        );
        level.send_particles(
            ParticleOptions::Block {
                particle_type: ParticleType::BLOCK_CRUMBLE,
                state: Block::CREAKING_HEART.default_state().set(crate::world::level::block::props::CREAKING_HEART_STATE, CreakingHeartState::Awake),
            },
            center.x,
            center.y,
            center.z,
            10,
            x_spread,
            y_spread,
            z_spread,
            0.0,
        );
        if let Some(sound) = self.get_death_sound() {
            damage::make_sound(self, level, sound);
        }
        self.remove(RemovalReason::Discarded);
    }

    /// `creakingDeathEffects(source)`.
    pub fn creaking_death_effects(&mut self, level: &mut ServerLevel, source: &DamageSource) {
        self.blame_source_for_damage(level, source);
        self.die(level, source);
        damage::make_sound(self, level, SoundEvent::ENTITY_CREAKING_TWITCH);
    }

    /// `playerIsStuckInYou()`.
    pub fn player_is_stuck_in_you(&mut self, level: &ServerLevel) -> bool {
        let players: Vec<EntityRef> = self.brain().get_memory(&NEAREST_PLAYERS).cloned().unwrap_or_default();
        if players.is_empty() {
            self.player_stuck_counter = 0;
            return false;
        }
        let own_box = self.get_bounding_box();
        for player in players {
            if level.get_entity(player.0).is_some_and(|p| own_box.contains(p.base().get_eye_position())) {
                self.player_stuck_counter += 1;
                return self.player_stuck_counter > MAX_PLAYER_STUCK_COUNTER;
            }
        }
        self.player_stuck_counter = 0;
        false
    }

    /// `checkCanMove()`.
    pub fn check_can_move(&mut self, level: &mut ServerLevel) -> bool {
        let players: Vec<EntityRef> = self.brain().get_memory(&NEAREST_PLAYERS).cloned().unwrap_or_default();
        let active = self.is_active();
        if players.is_empty() {
            if active {
                self.deactivate(level);
            }
            return true;
        }
        let mut has_potential_target = false;
        let mut to_activate = None;
        let mut frozen = false;
        for player_ref in &players {
            let Some(player) = level.get_entity(player_ref.0) else { continue };
            if self.can_attack(level, player) && !mob::ext::is_allied_to(level, self, player) {
                has_potential_target = true;
                let eye_y = self.base().get_eye_y();
                let y = self.base().y();
                let scale = self.mob.living.get_attribute_value(Attribute::SCALE);
                if (!active || mob::ext::player_not_wearing_disguise_item(player))
                    && mob::ext::is_looking_at_me(level, self, player, 0.5, false, true, &[eye_y, y + 0.5 * scale, (eye_y + y) / 2.0])
                {
                    if active {
                        frozen = true;
                        break;
                    }
                    if self.base().distance_to_sqr_vec(player.position()) < ACTIVATION_RANGE_SQ {
                        to_activate = Some(player_ref.0);
                        break;
                    }
                }
            }
        }
        if frozen {
            return false;
        }
        if let Some(player) = to_activate {
            self.activate(level, player);
            return false;
        }
        if !has_potential_target && active {
            self.deactivate(level);
        }
        true
    }

    /// `activate(player)`.
    pub fn activate(&mut self, level: &mut ServerLevel, player: EntityId) {
        self.brain_mut().set_memory(&ATTACK_TARGET, EntityRef(player));
        self.game_event(level, GameEvent::ENTITY_ACTION);
        damage::make_sound(self, level, SoundEvent::ENTITY_CREAKING_ACTIVATE);
        self.set_is_active(true);
    }

    /// `deactivate()`.
    pub fn deactivate(&mut self, level: &mut ServerLevel) {
        self.brain_mut().erase_memory(&ATTACK_TARGET);
        self.game_event(level, GameEvent::ENTITY_ACTION);
        damage::make_sound(self, level, SoundEvent::ENTITY_CREAKING_DEACTIVATE);
        self.set_is_active(false);
    }

    /// `makeSound(sound)` (public for the creaking heart).
    pub fn make_sound(&mut self, level: &mut ServerLevel, sound: SoundEvent) {
        damage::make_sound(self, level, sound);
    }
}

/// `Creaking.CreakingBodyRotationControl`.
#[derive(Default)]
struct CreakingBodyRotationControl {
    base: BodyRotationControlBase,
}

impl BodyRotationControl for CreakingBodyRotationControl {
    fn base(&self) -> &BodyRotationControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BodyRotationControlBase {
        &mut self.base
    }
    fn client_tick(&mut self, mob: &mut dyn Entity) {
        if can_move(mob) {
            base_client_tick(&mut self.base, mob);
        }
    }
}

/// `Creaking.CreakingJumpControl`.
#[derive(Default)]
struct CreakingJumpControl {
    base: JumpControlBase,
}

impl JumpControl for CreakingJumpControl {
    fn base(&self) -> &JumpControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut JumpControlBase {
        &mut self.base
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if can_move(mob) {
            jump_control::base_tick(&mut self.base, mob, level);
        } else {
            mob.set_jumping(level, false);
        }
    }
}

/// `Creaking.CreakingLookControl`.
#[derive(Default)]
struct CreakingLookControl {
    base: LookControlBase,
}

impl LookControl for CreakingLookControl {
    fn base(&self) -> &LookControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut LookControlBase {
        &mut self.base
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if can_move(mob) {
            look_control::default_tick(self, mob, level);
        }
    }
}

/// `Creaking.CreakingMoveControl`.
#[derive(Default)]
struct CreakingMoveControl {
    base: MoveControlBase,
}

impl MoveControl for CreakingMoveControl {
    fn base(&self) -> &MoveControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut MoveControlBase {
        &mut self.base
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if can_move(mob) {
            move_control::base_tick(&mut self.base, mob, level);
        }
    }
}

/// `Creaking.HomeNodeEvaluator extends WalkNodeEvaluator`.
struct HomeNodeEvaluator(EvaluatorState);

impl NodeEvaluator for HomeNodeEvaluator {
    fn state(&self) -> &EvaluatorState {
        &self.0
    }
    fn state_mut(&mut self) -> &mut EvaluatorState {
        &mut self.0
    }

    fn get_path_type(&mut self, env: &mut PathEnv<'_>, x: i32, y: i32, z: i32) -> PathType {
        let home_pos = env.mob.entity().and_then(|e| e.base().entity_data.try_get(HOME_POS)).flatten();
        match home_pos {
            None => default_get_path_type(self, env, x, y, z),
            Some(home_pos) => {
                let home_distance = home_pos.dist_sqr_xyz(x, y, z);
                if home_distance > MAX_DISTANCE_TO_HOME_SQ && home_distance >= home_pos.dist_sqr(&env.context.mob_position()) {
                    PathType::BLOCKED
                } else {
                    default_get_path_type(self, env, x, y, z)
                }
            }
        }
    }
}

/// `Creaking.CreakingPathNavigation`.
pub struct CreakingNavigation {
    data: NavigationData,
}

impl CreakingNavigation {
    fn new(creaking: &Creaking) -> Self {
        let data = NavigationData::with_path_finder(creaking, NavClass::Ground, |max_visited_nodes| {
            let mut evaluator = HomeNodeEvaluator(EvaluatorState::walk());
            evaluator.set_can_pass_doors(true);
            PathFinder::new(Box::new(evaluator), max_visited_nodes)
        });
        CreakingNavigation { data }
    }
}

impl PathNavigation for CreakingNavigation {
    fn data(&self) -> &NavigationData {
        &self.data
    }
    fn data_mut(&mut self) -> &mut NavigationData {
        &mut self.data
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &crate::server::level::ServerLevel) {
        if can_move(mob) {
            crate::world::entity::ai::navigation::default_tick(self, mob, level);
        }
    }
}

impl Entity for Creaking {
    crate::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:creaking")
    }

    fn make_brain(&mut self, packed: &BrainPacked) -> Brain {
        BRAIN_PROVIDER.make_brain(self, packed)
    }

    fn do_hurt_target(&mut self, level: &mut ServerLevel, target: &mut dyn Entity) -> bool {
        if target.living().is_none() {
            return false;
        }
        self.attack_animation_remaining_ticks = ATTACK_ANIMATION_DURATION;
        level.broadcast_entity_event(self.id(), 4);
        mob::combat::mob_do_hurt_target(self, level, target)
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage_amount: f32) -> bool {
        let Some(home_pos) = self.get_home_pos().filter(|_| !source.is_tag(level, DamageTypeTags::BYPASSES_INVULNERABILITY)) else {
            return damage::hurt_server(self, level, source, damage_amount);
        };
        if !self.is_invulnerable_to(level, source) && self.invulnerability_animation_remaining_ticks <= 0 && !self.is_dead_or_dying() {
            let responsible_player = self.blame_source_for_damage(level, source);
            let direct_is_living_or_projectile = source.get_direct_entity().is_some_and(|id| {
                level.get_entity(id).map_or_else(|| source.direct_info().is_some_and(|d| d.is_living), |e| e.living().is_some() || e.as_projectile().is_some())
            });
            if !direct_is_living_or_projectile && responsible_player.is_none() {
                false
            } else {
                self.invulnerability_animation_remaining_ticks = INVULNERABILITY_ANIMATION_DURATION;
                level.broadcast_entity_event(self.id(), 66);
                self.game_event(level, GameEvent::ENTITY_ACTION);
                if self.has_protection_from_creaking_heart(level, home_pos) {
                    if responsible_player.is_some() {
                        level.with_block_entity(home_pos, |level, be| {
                            if let Some(heart) = be.as_any_mut().downcast_mut::<CreakingHeartBlockEntity>() {
                                heart.creaking_hurt(level, &*self);
                            }
                        });
                    }
                    // `playHurtSound(source)`
                    if let Some(sound) = self.get_hurt_sound(source) {
                        damage::make_sound(self, level, sound);
                    }
                }
                true
            }
        } else {
            false
        }
    }

    fn is_pushable(&self, level: &ServerLevel) -> bool {
        crate::world::entity::living_entity::push::living_is_pushable(self, level) && self.can_move()
    }

    fn push_xyz(&mut self, xa: f64, ya: f64, za: f64) {
        if self.can_move() && xa.is_finite() && ya.is_finite() && za.is_finite() {
            let m = self.base().delta_movement();
            self.base_mut().set_delta_movement(m.add(Vec3::new(xa, ya, za)));
            self.base_mut().needs_sync = true;
        }
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        Brain::tick(level, self);
        creaking_ai::update_activity(self);
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        if self.invulnerability_animation_remaining_ticks > 0 {
            self.invulnerability_animation_remaining_ticks -= 1;
        }
        if self.attack_animation_remaining_ticks > 0 {
            self.attack_animation_remaining_ticks -= 1;
        }
        let can_move = self.base().entity_data.get(CAN_MOVE);
        let now_can_move = self.check_can_move(level);
        if now_can_move != can_move {
            self.game_event(level, GameEvent::ENTITY_ACTION);
            if now_can_move {
                damage::make_sound(self, level, SoundEvent::ENTITY_CREAKING_UNFREEZE);
            } else {
                mob::tick::stop_in_place(self, level);
                damage::make_sound(self, level, SoundEvent::ENTITY_CREAKING_FREEZE);
            }
        }
        self.base_mut().entity_data.set(CAN_MOVE, now_can_move);
        mob::tick::ai_step(self, level);
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        if let Some(home_pos) = self.get_home_pos() {
            let has_protection_from_creaking_heart = self.has_protection_from_creaking_heart(level, home_pos);
            if !has_protection_from_creaking_heart {
                self.set_health(0.0);
            }
        }
        mob::tick::tick(self, level);
    }

    fn tick_death(&mut self, level: &mut ServerLevel) {
        if self.is_heart_bound() && self.is_tearing_down() {
            self.mob.living.death_time += 1;
            if self.mob.living.death_time > TWITCH_DEATH_DURATION && !self.is_removed() {
                self.tear_down(level);
            }
        } else {
            crate::world::entity::living_entity::base_tick::tick_death(self, level);
        }
    }

    fn fire_immune(&self) -> bool {
        self.is_heart_bound() || self.base().entity_type.data().fire_immune
    }

    fn can_use_portal(&self, ignore_passenger: bool) -> bool {
        !self.is_heart_bound() && (ignore_passenger || !self.base().is_passenger()) && self.is_alive()
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        if let Some(pos) = input.get_int_array("home_pos").filter(|a| a.len() == 3) {
            self.set_transient(BlockPos::new(pos[0], pos[1], pos[2]));
        }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        if let Some(pos) = self.get_home_pos() {
            out.put("home_pos", Tag::IntArray(vec![pos.x(), pos.y(), pos.z()]));
        }
    }

    fn play_attack_sound(&mut self, level: &mut ServerLevel) {
        damage::make_sound(self, level, SoundEvent::ENTITY_CREAKING_ATTACK);
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        if self.is_active() { None } else { Some(SoundEvent::ENTITY_CREAKING_AMBIENT) }
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(if self.is_heart_bound() { SoundEvent::ENTITY_CREAKING_SWAY } else { mob::monster::HURT_SOUND })
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_CREAKING_DEATH)
    }

    fn play_step_sound(&mut self, level: &mut ServerLevel, _pos: BlockPos, _state: mc_data::block::BlockState) {
        self.play_sound(level, SoundEvent::ENTITY_CREAKING_STEP, 0.15, 1.0);
    }

    fn get_target(&self) -> Option<EntityId> {
        self.brain().get_memory(&ATTACK_TARGET).map(|r| r.0)
    }

    fn knockback(&mut self, power: f64, xd: f64, zd: f64, source: &DamageSource, damage_amount: f32, comes_from_effect: bool) {
        if self.can_move() {
            damage::knockback(self, power, xd, zd, source, damage_amount, comes_from_effect);
        }
    }

    fn get_walk_target_value(&self, _pos: BlockPos, _level: &dyn crate::world::level::LevelReader) -> f32 {
        0.0
    }

    fn get_sound_source(&self) -> mc_protocol::packets::play::SoundSource {
        mob::monster::SOUND_SOURCE
    }

    fn get_swim_sound(&self) -> SoundEvent {
        mob::monster::SWIM_SOUND
    }

    fn get_swim_splash_sound(&self) -> SoundEvent {
        mob::monster::SWIM_SPLASH_SOUND
    }

    fn get_fall_sounds(&self) -> (SoundEvent, SoundEvent) {
        mob::monster::FALL_SOUNDS
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static CREAKING: EntityRegistration = EntityRegistration { entity_type: EntityType::CREAKING, create: |ty, id, registries| Box::new(Creaking::new(ty, id, registries)) };
