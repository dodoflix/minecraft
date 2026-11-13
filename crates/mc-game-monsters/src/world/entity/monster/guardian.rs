//! `net.minecraft.world.entity.monster.Guardian` and `ElderGuardian` (a `Guardian` subclass: one struct serves both types).

use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{Attribute, EntityType, MobEffect, SoundEvent};
use mc_game_ai::world::entity::ai::goal::MoveTowardsRestrictionGoal;
use mc_game_core::define_accessor;
use mc_game_core::register_spawn_placement;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::entity::ai::control::move_control::{
    MoveControl, MoveControlBase, Operation,
};
use mc_game_core::world::entity::ai::goal::look_at_player::LookAtType;
use mc_game_core::world::entity::ai::goal::target::{NearestAttackableTargetGoal, TargetType};
use mc_game_core::world::entity::ai::goal::{
    Flag, Goal, GoalFlags, LookAtPlayerGoal, RandomLookAroundGoal, RandomStrollGoal,
};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::ai::targeting::TargetingConditions;
use mc_game_core::world::entity::effect::MobEffectInstance;
use mc_game_core::world::entity::entity::MovementEmission;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::monster::WithFlags;
use mc_game_core::world::entity::{Entity, EntityId, MoverType};
use mc_game_core::world::level::LevelReader;
use mc_game_core::world::level::material::FluidStateExt as _;
use mc_game_core::world::level::pathfinder::PathType;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_world::level_storage::Difficulty;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

define_accessor!(pub DATA_ID_MOVING: Boolean = Guardian);
define_accessor!(pub DATA_ID_ATTACK_TARGET: Int = Guardian);

const EFFECT_INTERVAL: i32 = 1200;
const EFFECT_RADIUS: f64 = 50.0;
const EFFECT_DURATION: i32 = 6000;
const EFFECT_AMPLIFIER: i32 = 2;
const EFFECT_DISPLAY_LIMIT: i32 = 1200;

/// `Guardian` / `ElderGuardian`.
pub struct Guardian {
    pub mob: MobData,
    /// `randomStrollGoal.trigger()` requests, consumed by the goal's next `canUse`.
    stroll_trigger: Arc<AtomicBool>,
}

impl Guardian {
    /// `new Guardian(type, level)` / `new ElderGuardian(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Monster, |b| {
            b.define(DATA_ID_MOVING, false);
            b.define(DATA_ID_ATTACK_TARGET, 0);
        });
        let mut guardian = Guardian {
            mob,
            stroll_trigger: Arc::new(AtomicBool::new(false)),
        };
        guardian.mob.xp_reward = 10;
        guardian.mob.set_pathfinding_malus(PathType::Water, 0.0);
        guardian.mob.move_control = Some(Box::new(GuardianMoveControl {
            base: MoveControlBase::default(),
        }));
        // `clientSideTailAnimation = random.nextFloat()`: the draw is part of the entity random stream
        let _ = guardian.mob.living.entity.random.next_float();
        let navigation = StandardPathNavigation::water_bound(&guardian);
        guardian.mob.set_navigation(Box::new(navigation));
        guardian.register_goals();
        if ty == EntityType::ELDER_GUARDIAN {
            guardian.mob.persistence_required = true;
        }
        guardian
    }

    fn is_elder(&self) -> bool {
        self.mob.living.entity.entity_type == EntityType::ELDER_GUARDIAN
    }

    /// `registerGoals()` (`ElderGuardian` raises the stroll interval to 400 right after construction).
    fn register_goals(&mut self) {
        let interval = if self.is_elder() { 400 } else { 80 };
        let look_at_player = LookAtPlayerGoal::new_player(self, 8.0);
        let look_at_guardian = LookAtPlayerGoal::with_probability(
            self,
            LookAtType::Class(Arc::new(|e: &dyn Entity| e.as_any().is::<Guardian>())),
            12.0,
            0.01,
        );
        let stroll = GuardianRandomStrollGoal {
            stroll: RandomStrollGoal::with_interval(1.0, interval),
            trigger: self.stroll_trigger.clone(),
        };
        let elder = self.is_elder();
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(
            4,
            Box::new(GuardianAttackGoal {
                attack_time: 0,
                elder,
                stroll_trigger: self.stroll_trigger.clone(),
            }),
        );
        goals.add_goal(
            5,
            Box::new(WithFlags::new(
                MoveTowardsRestrictionGoal::new(1.0),
                GoalFlags::of(&[Flag::Move, Flag::Look]),
            )),
        );
        goals.add_goal(7, Box::new(stroll));
        goals.add_goal(8, Box::new(look_at_player));
        goals.add_goal(8, Box::new(look_at_guardian));
        goals.add_goal(9, Box::new(RandomLookAroundGoal::new()));
        self.mob
            .target_selector
            .add_goal(1, Box::new(GuardianTargetGoal::new()));
    }

    /// `isMoving()`.
    pub fn is_moving(&self) -> bool {
        self.mob.living.entity.entity_data.get(DATA_ID_MOVING)
    }

    /// `setMoving(value)`.
    pub fn set_moving(&mut self, value: bool) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_ID_MOVING, value);
    }

    /// `getAttackDuration()`.
    pub fn get_attack_duration(&self) -> i32 {
        if self.is_elder() { 60 } else { 80 }
    }

    /// `setActiveAttackTarget(entityId)`.
    fn set_active_attack_target(&mut self, entity_id: i32) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_ID_ATTACK_TARGET, entity_id);
    }

    /// `hasActiveAttackTarget()`.
    pub fn has_active_attack_target(&self) -> bool {
        self.mob
            .living
            .entity
            .entity_data
            .get(DATA_ID_ATTACK_TARGET)
            != 0
    }
}

impl Entity for Guardian {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_ambient_sound_interval(&self) -> i32 {
        160
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        let in_water = self.is_in_water();
        Some(match (self.is_elder(), in_water) {
            (false, true) => SoundEvent::ENTITY_GUARDIAN_AMBIENT,
            (false, false) => SoundEvent::ENTITY_GUARDIAN_AMBIENT_LAND,
            (true, true) => SoundEvent::ENTITY_ELDER_GUARDIAN_AMBIENT,
            (true, false) => SoundEvent::ENTITY_ELDER_GUARDIAN_AMBIENT_LAND,
        })
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        let in_water = self.is_in_water();
        Some(match (self.is_elder(), in_water) {
            (false, true) => SoundEvent::ENTITY_GUARDIAN_HURT,
            (false, false) => SoundEvent::ENTITY_GUARDIAN_HURT_LAND,
            (true, true) => SoundEvent::ENTITY_ELDER_GUARDIAN_HURT,
            (true, false) => SoundEvent::ENTITY_ELDER_GUARDIAN_HURT_LAND,
        })
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        let in_water = self.is_in_water();
        Some(match (self.is_elder(), in_water) {
            (false, true) => SoundEvent::ENTITY_GUARDIAN_DEATH,
            (false, false) => SoundEvent::ENTITY_GUARDIAN_DEATH_LAND,
            (true, true) => SoundEvent::ENTITY_ELDER_GUARDIAN_DEATH,
            (true, false) => SoundEvent::ENTITY_ELDER_GUARDIAN_DEATH_LAND,
        })
    }

    fn get_movement_emission(&self) -> MovementEmission {
        MovementEmission::Events
    }

    fn get_walk_target_value(&self, pos: BlockPos, level: &dyn LevelReader) -> f32 {
        if level.get_fluid_state(pos).is_tag("water") {
            10.0 + level.get_pathfinding_cost_from_light_levels(pos)
        } else {
            mob::pathfinder::get_walk_target_value(self, pos, level)
        }
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        if self.is_alive() {
            if self.is_in_water() {
                self.base_mut().set_air_supply(300);
            } else if self.base().on_ground() {
                let random = &mut self.mob.living.entity.random;
                let (dx, dz) = (
                    (random.next_float() * 2.0 - 1.0) * 0.4,
                    (random.next_float() * 2.0 - 1.0) * 0.4,
                );
                let m = self.base().delta_movement();
                self.base_mut()
                    .set_delta_movement(m.add_xyz(f64::from(dx), 0.5, f64::from(dz)));
                let y_rot = self.base_mut().random.next_float() * 360.0;
                self.base_mut().set_y_rot(y_rot);
                mc_game_core::world::entity::movement::set_on_ground(self, &*level, false);
                self.base_mut().needs_sync = true;
            }
            if self.has_active_attack_target() {
                let head = self.get_y_head_rot();
                self.base_mut().set_y_rot(head);
            }
        }
        mob::tick::ai_step(self, level);
    }

    fn check_spawn_obstruction(&self, level: &dyn ServerLevelAccessor) -> bool {
        level.is_unobstructed_entity(self)
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        if !self.is_moving()
            && !source.is_tag(level, "avoids_guardian_thorns")
            && !source.is("minecraft:thorns")
            && let Some(cause) = source.get_direct_entity().filter(|id| {
                level.get_entity(*id).is_some_and(|e| e.living().is_some())
                    || source.direct_info().is_some_and(|d| d.is_living)
            })
        {
            let thorns = level.damage_sources().thorns(self);
            level.lend(&mut *self, |level| {
                level.with_entity(cause, move |level, e| {
                    e.hurt_server(level, &thorns, 2.0);
                })
            });
        }
        self.stroll_trigger.store(true, Ordering::Relaxed);
        mc_game_core::world::entity::living_entity::damage::hurt_server(self, level, source, damage)
    }

    fn get_max_head_x_rot(&self) -> i32 {
        180
    }

    fn travel_in_water(
        &mut self,
        level: &mut ServerLevel,
        input: Vec3,
        _base_gravity: f64,
        _is_falling: bool,
        _old_y: f64,
    ) {
        self.move_relative(0.1, input);
        let m = self.base().delta_movement();
        self.r#move(level, MoverType::SelfMover, m);
        let m = self.base().delta_movement();
        self.base_mut().set_delta_movement(m.scale(0.9));
        if !self.is_moving() && self.get_target().is_none() {
            let m = self.base().delta_movement();
            self.base_mut()
                .set_delta_movement(m.add_xyz(0.0, -0.005, 0.0));
        }
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        if !self.is_elder() {
            return;
        }
        if (self.base().tick_count + self.id()) % EFFECT_INTERVAL == 0 {
            let mining_fatigue =
                MobEffectInstance::of(MobEffect::MINING_FATIGUE, EFFECT_DURATION, EFFECT_AMPLIFIER);
            let position = self.position();
            let affected =
                mc_game_core::world::effect::mob_effect_util::add_effect_to_players_around(
                    level,
                    Some(self.as_dyn()),
                    position,
                    EFFECT_RADIUS,
                    &mining_fatigue,
                    EFFECT_DISPLAY_LIMIT,
                );
            let param = if self.base().is_silent() { 0.0 } else { 1.0 };
            for player in affected {
                if let Some(connection) = level
                    .get_entity(player)
                    .and_then(|p| {
                        p.as_any()
                            .downcast_ref::<mc_game_core::world::entity::ServerPlayer>()
                    })
                    .and_then(|p| p.connection.as_ref())
                {
                    connection.send(&mc_protocol::packets::play::ClientboundGameEventPacket { event: mc_protocol::packets::play::ClientboundGameEventPacket::GUARDIAN_ELDER_EFFECT, param });
                }
            }
        }
        if !self.mob.has_home() {
            let pos = self.block_position();
            self.mob.set_home_to(pos, 16);
        }
    }
}

/// `Guardian.checkGuardianSpawnRules`.
fn check_guardian_spawn_rules(
    _ty: EntityType,
    level: &dyn ServerLevelAccessor,
    reason: EntitySpawnReason,
    pos: BlockPos,
    random: &mut dyn RandomSource,
) -> bool {
    (random.next_int_bounded(20) == 0 || !can_see_sky_from_below_water(level, pos))
        && level.get_difficulty() != Difficulty::Peaceful
        && (mc_game_core::world::entity::spawn_predicates::is_spawner(reason)
            || level.get_fluid_state(pos).is_tag("water"))
        && level.get_fluid_state(pos.below()).is_tag("water")
}

register_spawn_placement!(EntityType::GUARDIAN, check_guardian_spawn_rules);
register_spawn_placement!(EntityType::ELDER_GUARDIAN, check_guardian_spawn_rules);

/// `LevelReader.canSeeSkyFromBelowWater(pos)`.
fn can_see_sky_from_below_water(level: &dyn ServerLevelAccessor, pos: BlockPos) -> bool {
    if pos.y() >= level.get_sea_level() {
        return level.can_see_sky(pos);
    }
    let scan_point = BlockPos::new(pos.x(), level.get_sea_level(), pos.z());
    if !level.can_see_sky(scan_point) {
        return false;
    }
    let mut cur = scan_point.below();
    while cur.y() > pos.y() {
        let state = level.get_block_state(cur);
        if state.get_light_block() > 0 && !state.liquid() {
            return false;
        }
        cur = cur.below();
    }
    true
}

/// `RandomStrollGoal(this, 1.0, 80)` with `setFlags(MOVE, LOOK)`; `trigger()` is requested through `trigger`.
struct GuardianRandomStrollGoal {
    stroll: RandomStrollGoal,
    trigger: Arc<AtomicBool>,
}

impl Goal for GuardianRandomStrollGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if self.trigger.swap(false, Ordering::Relaxed) {
            self.stroll.trigger();
        }
        self.stroll.can_use(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.stroll.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.stroll.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.stroll.stop(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}

fn guardian_mut(mob: &mut dyn Entity) -> &mut Guardian {
    mob.as_any_mut()
        .downcast_mut::<Guardian>()
        .expect("Guardian")
}

/// `Guardian.GuardianAttackGoal`.
struct GuardianAttackGoal {
    attack_time: i32,
    elder: bool,
    stroll_trigger: Arc<AtomicBool>,
}

impl Goal for GuardianAttackGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        mob::tick::get_target_checked(mob, level)
            .and_then(|t| level.get_entity(t))
            .is_some_and(|t| t.is_alive())
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        // `super.canContinueToUse()` = `canUse()`
        self.can_use(mob, level)
            && (self.elder
                || mob::tick::get_target_checked(mob, level)
                    .and_then(|t| level.get_entity(t))
                    .is_some_and(|t| mob.base().distance_to_sqr_vec(t.position()) > 9.0))
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.attack_time = -10;
        mob::navigation::stop(mob, level);
        if let Some(target) =
            mob::tick::get_target_checked(mob, level).and_then(|t| level.get_entity(t))
        {
            mob::look_at_entity_full(mob, target, 90.0, 90.0);
        }
        mob.base_mut().needs_sync = true;
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        guardian_mut(mob).set_active_attack_target(0);
        mob.set_target(level, None);
        self.stroll_trigger.store(true, Ordering::Relaxed);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(target_id) = mob::tick::get_target_checked(mob, level) else {
            return;
        };
        mob::navigation::stop(mob, level);
        {
            let Some(target) = level.get_entity(target_id) else {
                return;
            };
            mob::look_at_entity_full(mob, target, 90.0, 90.0);
        }
        let can_see = level
            .get_entity(target_id)
            .is_some_and(|t| mob::ext::has_line_of_sight(level, mob, t));
        if !can_see {
            mob.set_target(level, None);
            return;
        }
        self.attack_time += 1;
        let duration = guardian_mut(mob).get_attack_duration();
        if self.attack_time == 0 {
            guardian_mut(mob).set_active_attack_target(target_id);
            if !mob.base().is_silent() {
                level.broadcast_entity_event(mob.id(), 21);
            }
        } else if self.attack_time >= duration {
            let mut magic_damage = 1.0f32;
            if level.difficulty() == Difficulty::Hard {
                magic_damage += 2.0;
            }
            if self.elder {
                magic_damage += 2.0;
            }
            let source = level
                .damage_sources()
                .indirect_magic(mob.as_dyn(), Some(mob.as_dyn()));
            level.with_entity(target_id, |level, target| {
                level.lend(&mut *mob, |level| {
                    target.hurt_server(level, &source, magic_damage)
                });
            });
            mob::combat::do_hurt_target(mob, level, target_id);
            mob.set_target(level, None);
        }
        // `super.tick()` (Goal.tick) does nothing
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}

/// `NearestAttackableTargetGoal<>(this, LivingEntity.class, 10, true, false, new GuardianAttackSelector(this))`: the selector
/// reads the guardian's position, so it is rebuilt from the mob before each use.
struct GuardianTargetGoal {
    inner: NearestAttackableTargetGoal,
}

impl GuardianTargetGoal {
    fn new() -> Self {
        let class: mc_game_core::world::entity::ai::goal::target::EntityClass =
            Arc::new(|e: &dyn Entity| e.living().is_some());
        Self {
            inner: NearestAttackableTargetGoal::build(
                TargetType::Class(class),
                10,
                true,
                false,
                None,
            ),
        }
    }
}

impl Goal for GuardianTargetGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let position = mob.position();
        let selector = Arc::new(move |target: &dyn Entity, _level: &ServerLevel| {
            (target.is_player()
                || matches!(target.get_type(), EntityType::SQUID | EntityType::AXOLOTL))
                && target.base().distance_to_sqr_vec(position) > 9.0
        });
        self.inner.target_conditions = TargetingConditions::for_combat().selector(Some(selector));
        self.inner.can_use(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.stop(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }
}

/// `Guardian.GuardianMoveControl`.
struct GuardianMoveControl {
    base: MoveControlBase,
}

impl MoveControl for GuardianMoveControl {
    fn base(&self) -> &MoveControlBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut MoveControlBase {
        &mut self.base
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if self.base.operation == Operation::MoveTo && !mob::navigation::is_done(mob, level) {
            let delta = Vec3::new(
                self.base.wanted_x - mob.base().x(),
                self.base.wanted_y - mob.base().y(),
                self.base.wanted_z - mob.base().z(),
            );
            let length = delta.length();
            let xd = delta.x / length;
            let yd = delta.y / length;
            let zd = delta.z / length;
            let pi = std::f32::consts::PI;
            let y_rot_d = (mth::atan2(delta.z, delta.x) * f64::from(mc_core::math::mth::RAD_TO_DEG))
                as f32
                - 90.0;
            let new_y_rot = self.base.rotlerp(mob.base().y_rot(), y_rot_d, 90.0);
            mob.base_mut().set_y_rot(new_y_rot);
            mob.living_mut().expect("living").y_body_rot = new_y_rot;
            let target_speed = (self.base.speed_modifier
                * mob::attribute_value(mob, Attribute::MOVEMENT_SPEED))
                as f32;
            let new_speed = mth::lerp_f32(0.125, mob.get_speed(), target_speed);
            mob.set_speed(new_speed);
            let tick_id = f64::from(mob.base().tick_count + mob.id());
            let push = (tick_id * 0.5).sin() * 0.05;
            let y_rot = mob.base().y_rot();
            let cos = f64::from(y_rot * (pi / 180.0)).cos();
            let sin = f64::from(y_rot * (pi / 180.0)).sin();
            let y_push = (tick_id * 0.75).sin() * 0.05;
            let m = mob.base().delta_movement();
            mob.base_mut().set_delta_movement(m.add_xyz(
                push * cos,
                y_push * (sin + cos) * 0.25 + f64::from(new_speed) * yd * 0.1,
                push * sin,
            ));
            let (old_x, old_y, old_z, looking) = {
                let control = mob
                    .mob()
                    .and_then(|m| m.look_control.as_deref())
                    .expect("look control");
                (
                    control.get_wanted_x(),
                    control.get_wanted_y(),
                    control.get_wanted_z(),
                    control.is_looking_at_target(),
                )
            };
            let new_look_x = mob.base().x() + xd * 2.0;
            let new_look_y = mob.base().get_eye_y() + yd / length;
            let new_look_z = mob.base().z() + zd * 2.0;
            let (old_x, old_y, old_z) = if !looking {
                (new_look_x, new_look_y, new_look_z)
            } else {
                (old_x, old_y, old_z)
            };
            if let Some(m) = mob.mob_mut() {
                m.look_control_mut().set_look_at_full(
                    mth::lerp_f64(0.125, old_x, new_look_x),
                    mth::lerp_f64(0.125, old_y, new_look_y),
                    mth::lerp_f64(0.125, old_z, new_look_z),
                    10.0,
                    40.0,
                );
            }
            guardian_mut(mob).set_moving(true);
        } else {
            mob.set_speed(0.0);
            guardian_mut(mob).set_moving(false);
        }
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static GUARDIAN: EntityRegistration = EntityRegistration {
    entity_type: EntityType::GUARDIAN,
    create: |ty, id, registries| Box::new(Guardian::new(ty, id, registries)),
};

#[distributed_slice(ENTITY_FACTORIES)]
static ELDER_GUARDIAN: EntityRegistration = EntityRegistration {
    entity_type: EntityType::ELDER_GUARDIAN,
    create: |ty, id, registries| Box::new(Guardian::new(ty, id, registries)),
};
