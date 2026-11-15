//! `net.minecraft.world.entity.boss.wither.WitherBoss`: the flying three-headed boss. It charges up invulnerable (a boss bar that
//! fills, a blast when it ends), shoots wither skulls with its heads at the targets they pick, breaks the blocks around it when
//! hurt and drops a nether star.

use crate::commands::host;
use crate::define_accessor;
use crate::registry::RegistryAccess;
use crate::server::bossevents::{BossBarColor, BossBarOverlay};
use crate::server::level::ServerLevel;
use crate::server::level::level_explode::Explode;
use crate::server::level::server_boss_event::ServerBossEvent;
use crate::server::network::ConnectionSender;
use crate::world::damagesource::display_name::entity_display_name;
use crate::world::damagesource::{DamageSource, DamageTypeTags};
use crate::world::entity::ai::control::FlyingMoveControl;
use crate::world::entity::ai::goal::target::{
    HurtByTargetGoal, NearestAttackableTargetGoal, TargetType,
};
use crate::world::entity::ai::goal::{
    Flag, Goal, GoalFlags, LookAtPlayerGoal, RandomLookAroundGoal, RangedAttackGoal,
    WaterAvoidingRandomFlyingGoal,
};
use crate::world::entity::ai::navigation::{PathNavigation, StandardPathNavigation};
use crate::world::entity::ai::targeting::TargetingConditions;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::item::{ItemEntity, spawn_at_location};
use crate::world::entity::mob::{self, MobData, MobKind};
use crate::world::entity::projectile::hurting::wither_skull::{WitherSkull, can_destroy};
use crate::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use crate::world::entity::{Entity, EntityId};
use crate::world::item::ItemStack;
use crate::world::level::explosion::ExplosionInteraction;
use crate::world::level::{BlockGetter as _, LevelAccessor as _};
use mc_core::math::java::java_float_to_string;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_data::{EntityType, Item, MobEffect, SoundEvent};
use mc_nbt::CompoundTag;
use mc_text::Component;
use mc_world::level_storage::Difficulty;
use std::sync::Arc;

define_accessor!(pub DATA_TARGET_A: Int = WitherBoss);
define_accessor!(pub DATA_TARGET_B: Int = WitherBoss);
define_accessor!(pub DATA_TARGET_C: Int = WitherBoss);
define_accessor!(pub DATA_ID_INV: Int = WitherBoss);

const DATA_TARGETS: [crate::world::entity::synched_data::EntityDataAccessor<
    crate::world::entity::synched_data::serializer::Int,
>; 3] = [DATA_TARGET_A, DATA_TARGET_B, DATA_TARGET_C];

/// `INVULNERABLE_TICKS`.
const INVULNERABLE_TICKS: i32 = 220;

/// `WitherBoss`.
pub struct WitherBoss {
    pub mob: MobData,
    x_rot_heads: [f32; 2],
    y_rot_heads: [f32; 2],
    x_rot_o_heads: [f32; 2],
    y_rot_o_heads: [f32; 2],
    next_head_update: [i32; 2],
    idle_head_updates: [i32; 2],
    destroy_blocks_tick: i32,
    boss_event: ServerBossEvent,
}

/// `TARGETING_CONDITIONS`: `forCombat().range(20.0).selector(LIVING_ENTITY_SELECTOR)`.
fn targeting_conditions() -> TargetingConditions {
    TargetingConditions::for_combat()
        .range(20.0)
        .selector(Some(Arc::new(living_entity_selector)))
}

/// `LIVING_ENTITY_SELECTOR`: `!target.is(EntityTypeTags.WITHER_FRIENDS) && target.attackable()`.
fn living_entity_selector(target: &dyn Entity, _level: &ServerLevel) -> bool {
    !is_wither_friend(target.get_type()) && target.attackable()
}

/// `EntityTypeTags.WITHER_FRIENDS`.
fn is_wither_friend(ty: EntityType) -> bool {
    mc_data::tags::get::<EntityType>("wither_friends").is_some_and(|tag| tag.contains(ty))
}

/// `instanceof AbstractArrow`: arrows, spectral arrows and tridents.
fn is_abstract_arrow(ty: EntityType) -> bool {
    matches!(
        ty,
        EntityType::ARROW | EntityType::SPECTRAL_ARROW | EntityType::TRIDENT
    )
}

/// `WitherBoss.rotlerp(a, b, max)`.
fn rotlerp(a: f32, b: f32, max: f32) -> f32 {
    let mut diff = mth::wrap_degrees_f32(b - a);
    if diff > max {
        diff = max;
    }
    if diff < -max {
        diff = -max;
    }
    a + diff
}

impl WitherBoss {
    /// `new WitherBoss(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Monster, |b| {
            b.define(DATA_TARGET_A, 0);
            b.define(DATA_TARGET_B, 0);
            b.define(DATA_TARGET_C, 0);
            b.define(DATA_ID_INV, 0);
        });
        let mut wither = WitherBoss {
            boss_event: ServerBossEvent::new(
                uuid::Uuid::nil(),
                Component::empty(),
                BossBarColor::Purple,
                BossBarOverlay::Progress,
            ),
            mob,
            x_rot_heads: [0.0; 2],
            y_rot_heads: [0.0; 2],
            x_rot_o_heads: [0.0; 2],
            y_rot_o_heads: [0.0; 2],
            next_head_update: [0; 2],
            idle_head_updates: [0; 2],
            destroy_blocks_tick: 0,
        };
        // the field initializer `new ServerBossEvent(Mth.createInsecureUUID(this.random), this.getDisplayName(), ...)`
        let event_id = mth::create_insecure_uuid(&mut wither.mob.living.entity.random);
        let name = host::decorate_display_name(host::entity_name(&wither).copy(), &wither);
        wither.boss_event = ServerBossEvent::new(
            event_id,
            name,
            BossBarColor::Purple,
            BossBarOverlay::Progress,
        );
        wither.boss_event.set_darken_screen(true);
        let mut navigation = StandardPathNavigation::flying(&wither);
        navigation.set_can_open_doors(false);
        navigation.set_can_float(true);
        wither.mob.set_navigation(Box::new(navigation));
        wither.register_goals();
        wither.mob.move_control = Some(Box::new(FlyingMoveControl::new(10, false)));
        let max_health = wither.mob.living.get_max_health();
        wither.mob.living.set_health(max_health);
        wither.mob.xp_reward = 50;
        wither
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let look_at = LookAtPlayerGoal::new_player(self, 8.0);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(0, Box::new(WitherDoNothingGoal));
        goals.add_goal(2, Box::new(RangedAttackGoal::new(1.0, 40, 20.0)));
        goals.add_goal(5, Box::new(WaterAvoidingRandomFlyingGoal::new(1.0)));
        goals.add_goal(6, Box::new(look_at));
        goals.add_goal(7, Box::new(RandomLookAroundGoal::new()));
        let targets = &mut self.mob.target_selector;
        targets.add_goal(1, Box::new(HurtByTargetGoal::new(Vec::new())));
        targets.add_goal(
            2,
            Box::new(NearestAttackableTargetGoal::build(
                TargetType::Class(Arc::new(|e| e.living().is_some())),
                0,
                false,
                false,
                Some(Arc::new(living_entity_selector)),
            )),
        );
    }

    /// `getDisplayName()`: the team is looked up when a level is at hand.
    fn display_name(&self, level: Option<&ServerLevel>) -> Component {
        match level {
            Some(level) => entity_display_name(level, self),
            None => host::decorate_display_name(host::entity_name(self).copy(), self),
        }
    }

    /// `getHeadX(index)`.
    fn head_x(&self, index: usize) -> f64 {
        if index == 0 {
            return self.base().x();
        }
        let head_angle =
            (self.mob.living.y_body_rot + (180 * (index as i32 - 1)) as f32) * mth::DEG_TO_RAD;
        let cos = mth::cos(f64::from(head_angle));
        self.base().x() + f64::from(cos) * 1.3 * f64::from(self.get_scale())
    }

    /// `getHeadY(index)`.
    fn head_y(&self, index: usize) -> f64 {
        let height: f32 = if index == 0 { 3.0 } else { 2.2 };
        self.base().y() + f64::from(height * self.get_scale())
    }

    /// `getHeadZ(index)`.
    fn head_z(&self, index: usize) -> f64 {
        if index == 0 {
            return self.base().z();
        }
        let head_angle =
            (self.mob.living.y_body_rot + (180 * (index as i32 - 1)) as f32) * mth::DEG_TO_RAD;
        let sin = mth::sin(f64::from(head_angle));
        self.base().z() + f64::from(sin) * 1.3 * f64::from(self.get_scale())
    }

    pub fn get_head_y_rots(&self) -> [f32; 2] {
        self.y_rot_heads
    }

    pub fn get_head_x_rots(&self) -> [f32; 2] {
        self.x_rot_heads
    }

    /// `getInvulnerableTicks()`.
    pub fn get_invulnerable_ticks(&self) -> i32 {
        self.base().entity_data.get(DATA_ID_INV)
    }

    /// `setInvulnerableTicks(invulnerableTicks)`.
    pub fn set_invulnerable_ticks(&mut self, invulnerable_ticks: i32) {
        self.base_mut()
            .entity_data
            .set(DATA_ID_INV, invulnerable_ticks);
    }

    /// `getAlternativeTarget(headIndex)`.
    pub fn get_alternative_target(&self, head_index: usize) -> i32 {
        self.base().entity_data.get(DATA_TARGETS[head_index])
    }

    /// `setAlternativeTarget(headIndex, entityId)`.
    pub fn set_alternative_target(&mut self, head_index: usize, entity_id: i32) {
        self.base_mut()
            .entity_data
            .set(DATA_TARGETS[head_index], entity_id);
    }

    /// `isPowered()`.
    pub fn is_powered(&self) -> bool {
        self.mob.living.get_health() <= self.mob.living.get_max_health() / 2.0
    }

    /// `makeInvulnerable()`.
    pub fn make_invulnerable(&mut self) {
        self.set_invulnerable_ticks(INVULNERABLE_TICKS);
        self.boss_event.set_progress(0.0);
        let health = self.mob.living.get_max_health() / 3.0;
        self.mob.living.set_health(health);
    }

    /// The particle arguments the server evaluates for `Level.addParticle` (a no-op there): the entity random draws stay.
    fn draw_particle_offsets(&mut self) {
        let random = &mut self.base_mut().random;
        random.next_gaussian();
        random.next_gaussian();
        random.next_gaussian();
    }

    /// `performRangedAttack(head, target)`.
    fn perform_head_attack(&mut self, level: &mut ServerLevel, head: usize, target: EntityId) {
        let Some((tx, ty, tz)) = level.get_entity(target).map(|t| {
            (
                t.base().x(),
                t.base().y() + f64::from(t.base().eye_height) * 0.5,
                t.base().z(),
            )
        }) else {
            return;
        };
        let dangerous = head == 0 && self.base_mut().random.next_float() < 0.001;
        self.perform_head_attack_at(level, head, tx, ty, tz, dangerous);
    }

    /// `performRangedAttack(head, tx, ty, tz, dangerous)`.
    fn perform_head_attack_at(
        &mut self,
        level: &mut ServerLevel,
        head: usize,
        tx: f64,
        ty: f64,
        tz: f64,
        dangerous: bool,
    ) {
        if !self.base().is_silent() {
            let pos = self.base().block_position();
            level.level_event(None, 1024, pos, 0);
        }
        let hx = self.head_x(head);
        let hy = self.head_y(head);
        let hz = self.head_z(head);
        let direction = Vec3::new(tx - hx, ty - hy, tz - hz);
        let mut skull = WitherSkull::from_mob(level, &*self, direction.normalize());
        if dangerous {
            skull.set_dangerous(true);
        }
        skull.set_pos(hx, hy, hz);
        level.add_fresh_entity(Box::new(skull));
    }

    /// The block breaking of `customServerAiStep` once `destroyBlocksTick` runs out.
    fn destroy_blocks_around(&mut self, level: &mut ServerLevel) {
        let width = mth::floor_f32(self.base().get_bb_width() / 2.0 + 1.0);
        let height = mth::floor_f32(self.base().get_bb_height());
        let (bx, by, bz) = (
            self.base().get_block_x(),
            self.base().get_block_y(),
            self.base().get_block_z(),
        );
        let id = self.id();
        let destroyed = level.lend(self.as_dyn_mut(), |level| {
            let mut destroyed = false;
            for pos in BlockPos::between_closed_xyz(
                bx - width,
                by,
                bz - width,
                bx + width,
                by + height,
                bz + width,
            ) {
                if can_destroy(level.get_block_state(pos)) {
                    destroyed = level.destroy_block_impl(pos, true, Some(id), 512) || destroyed;
                }
            }
            destroyed
        });
        if destroyed {
            let pos = self.base().block_position();
            level.level_event(None, 1022, pos, 0);
        }
    }
}

impl Entity for WitherBoss {
    crate::mob_entity_accessors!(mob);

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_int("Invul", self.get_invulnerable_ticks());
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, level, input);
        self.set_invulnerable_ticks(input.get_int_or("Invul", 0));
        if self.base().get_custom_name().is_some() {
            let name = self.display_name(level);
            self.boss_event.set_name(name);
        }
    }

    fn set_custom_name(&mut self, level: Option<&ServerLevel>, name: Option<Component>) {
        self.base_mut().set_custom_name(name);
        let name = self.display_name(level);
        self.boss_event.set_name(name);
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_WITHER_AMBIENT)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_WITHER_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_WITHER_DEATH)
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        let mut delta_movement = self.base().delta_movement().multiply_xyz(1.0, 0.6, 1.0);
        if self.get_alternative_target(0) > 0
            && let Some((ty, tx, tz)) = level
                .get_entity(self.get_alternative_target(0))
                .map(|e| (e.base().y(), e.base().x(), e.base().z()))
        {
            let mut yd = delta_movement.y;
            if self.base().y() < ty || !self.is_powered() && self.base().y() < ty + 5.0 {
                yd = yd.max(0.0);
                yd += 0.3 - yd * f64::from(0.6f32);
            }
            delta_movement = Vec3::new(delta_movement.x, yd, delta_movement.z);
            let delta = Vec3::new(tx - self.base().x(), 0.0, tz - self.base().z());
            if delta.horizontal_distance_sqr() > 9.0 {
                let scale = delta.normalize();
                delta_movement = delta_movement.add_xyz(
                    scale.x * 0.3 - delta_movement.x * 0.6,
                    0.0,
                    scale.z * 0.3 - delta_movement.z * 0.6,
                );
            }
        }
        self.base_mut().set_delta_movement(delta_movement);
        if delta_movement.horizontal_distance_sqr() > 0.05 {
            let y_rot = mth::atan2(delta_movement.z, delta_movement.x) as f32 * 57.295776f32 - 90.0;
            self.base_mut().set_y_rot(y_rot);
        }
        mob::tick::ai_step(self, level);
        for i in 0..2 {
            self.y_rot_o_heads[i] = self.y_rot_heads[i];
            self.x_rot_o_heads[i] = self.x_rot_heads[i];
        }
        for i in 0..2 {
            let entity_id = self.get_alternative_target(i + 1);
            let target = (entity_id > 0)
                .then(|| level.get_entity(entity_id))
                .flatten()
                .map(|e| (e.base().x(), e.base().get_eye_y(), e.base().z()));
            if let Some((tx, ty, tz)) = target {
                let hx = self.head_x(i + 1);
                let hy = self.head_y(i + 1);
                let hz = self.head_z(i + 1);
                let xd = tx - hx;
                let ydx = ty - hy;
                let zd = tz - hz;
                let sd = (xd * xd + zd * zd).sqrt();
                let y_rot_d = (mth::atan2(zd, xd) * 57.2957763671875) as f32 - 90.0;
                let x_rot_d = (-(mth::atan2(ydx, sd) * 57.2957763671875)) as f32;
                self.x_rot_heads[i] = rotlerp(self.x_rot_heads[i], x_rot_d, 40.0);
                self.y_rot_heads[i] = rotlerp(self.y_rot_heads[i], y_rot_d, 10.0);
            } else {
                let y_body_rot = self.mob.living.y_body_rot;
                self.y_rot_heads[i] = rotlerp(self.y_rot_heads[i], y_body_rot, 10.0);
            }
        }
        let is_powered = self.is_powered();
        for _ in 0..3 {
            // `addParticle` does nothing on the server, but its arguments draw from the random sources
            self.draw_particle_offsets();
            if is_powered && level.random().next_int_bounded(4) == 0 {
                self.draw_particle_offsets();
            }
        }
        if self.get_invulnerable_ticks() > 0 {
            for _ in 0..3 {
                let random = &mut self.base_mut().random;
                random.next_gaussian();
                random.next_float();
                random.next_gaussian();
            }
        }
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        if self.get_invulnerable_ticks() > 0 {
            let new_count = self.get_invulnerable_ticks() - 1;
            self.boss_event
                .set_progress(1.0 - new_count as f32 / 220.0f32);
            if new_count <= 0 {
                let (x, eye_y, z) = (self.base().x(), self.base().get_eye_y(), self.base().z());
                level.explode(Explode::new(
                    Some(self.as_dyn_mut()),
                    x,
                    eye_y,
                    z,
                    7.0,
                    ExplosionInteraction::Mob,
                ));
                if !self.base().is_silent() {
                    let pos = self.base().block_position();
                    level.global_level_event(1023, pos, 0);
                }
            }
            self.set_invulnerable_ticks(new_count);
            if self.base().tick_count % 10 == 0 {
                self.heal(level, 10.0);
            }
        } else {
            for i in 1..3 {
                if self.base().tick_count >= self.next_head_update[i - 1] {
                    self.next_head_update[i - 1] =
                        self.base().tick_count + 10 + self.base_mut().random.next_int_bounded(10);
                    if matches!(level.difficulty(), Difficulty::Normal | Difficulty::Hard) {
                        let idle = self.idle_head_updates[i - 1];
                        self.idle_head_updates[i - 1] = idle + 1;
                        if idle > 15 {
                            let (x, y, z) = (self.base().x(), self.base().y(), self.base().z());
                            let random = &mut self.base_mut().random;
                            let xt = mth::next_double(random, x - 10.0, x + 10.0);
                            let yt = mth::next_double(random, y - 5.0, y + 5.0);
                            let zt = mth::next_double(random, z - 10.0, z + 10.0);
                            self.perform_head_attack_at(level, i + 1, xt, yt, zt, true);
                            self.idle_head_updates[i - 1] = 0;
                        }
                    }
                    let head_target = self.get_alternative_target(i);
                    if head_target > 0 {
                        let current = level
                            .get_entity(head_target)
                            .filter(|c| c.living().is_some());
                        let attackable = current.is_some_and(|c| {
                            self.can_attack(level, c)
                                && !(self.base().distance_to_sqr_vec(c.position()) > 900.0)
                                && mob::ext::has_line_of_sight(level, self, c)
                        });
                        if attackable {
                            self.perform_head_attack(level, i + 1, head_target);
                            self.next_head_update[i - 1] = self.base().tick_count
                                + 40
                                + self.base_mut().random.next_int_bounded(20);
                            self.idle_head_updates[i - 1] = 0;
                        } else {
                            self.set_alternative_target(i, 0);
                        }
                    } else {
                        let bb = self.base().bounding_box().inflate(20.0, 8.0, 20.0);
                        let entities = mob::ext::get_nearby_entities(
                            level,
                            |e| e.living().is_some(),
                            &targeting_conditions(),
                            &*self,
                            &bb,
                        );
                        if !entities.is_empty() {
                            let selected = entities[self
                                .base_mut()
                                .random
                                .next_int_bounded(entities.len() as i32)
                                as usize];
                            self.set_alternative_target(i, selected);
                        }
                    }
                }
            }
            match mob::tick::get_target_checked(self, level) {
                Some(target) => self.set_alternative_target(0, target),
                None => self.set_alternative_target(0, 0),
            }
            if self.destroy_blocks_tick > 0 {
                self.destroy_blocks_tick -= 1;
                if self.destroy_blocks_tick == 0 && level.mob_griefing() {
                    self.destroy_blocks_around(level);
                }
            }
            if self.base().tick_count % 20 == 0 {
                self.heal(level, 1.0);
            }
            let progress = self.mob.living.get_health() / self.mob.living.get_max_health();
            self.boss_event.set_progress(progress);
        }
    }

    fn make_stuck_in_block(
        &mut self,
        _level: &mut ServerLevel,
        _block_state: BlockState,
        _speed_multiplier: Vec3,
    ) {
    }

    fn start_seen_by_player(&mut self, player_id: EntityId, connection: &ConnectionSender) {
        self.boss_event.add_player(player_id, connection);
    }

    fn stop_seen_by_player(&mut self, player_id: EntityId) {
        self.boss_event.remove_player(player_id);
    }

    fn perform_ranged_attack(&mut self, level: &mut ServerLevel, target: EntityId, _power: f32) {
        self.perform_head_attack(level, 0, target);
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, amount: f32) -> bool {
        if self.is_invulnerable_to(level, source) {
            return false;
        }
        if source.is_tag(level, DamageTypeTags::WITHER_IMMUNE_TO)
            || source
                .causing_info()
                .is_some_and(|e| e.entity_type == EntityType::WITHER)
        {
            return false;
        }
        if self.get_invulnerable_ticks() > 0
            && !source.is_tag(level, DamageTypeTags::BYPASSES_INVULNERABILITY)
        {
            return false;
        }
        if self.is_powered()
            && source.direct_info().is_some_and(|e| {
                is_abstract_arrow(e.entity_type) || e.entity_type == EntityType::WIND_CHARGE
            })
        {
            return false;
        }
        if source
            .causing_info()
            .is_some_and(|e| is_wither_friend(e.entity_type))
        {
            return false;
        }
        if self.destroy_blocks_tick <= 0 {
            self.destroy_blocks_tick = 20;
        }
        for idle in &mut self.idle_head_updates {
            *idle += 3;
        }
        crate::world::entity::living_entity::damage::hurt_server(
            self.as_dyn_mut(),
            level,
            source,
            amount,
        )
    }

    fn drop_custom_death_loot(
        &mut self,
        level: &mut ServerLevel,
        source: &DamageSource,
        killed_by_player: bool,
    ) {
        mob::equipment::drop_custom_death_loot(self.as_dyn_mut(), level, source, killed_by_player);
        let nether_star =
            spawn_at_location(level, &*self, ItemStack::new(Item::NETHER_STAR), Vec3::ZERO);
        if let Some(id) = nether_star {
            level.with_entity(id, |_, e| {
                if let Some(item) = e.as_any_mut().downcast_mut::<ItemEntity>() {
                    item.set_extended_lifetime();
                }
            });
        }
    }

    fn check_despawn(&mut self, level: &mut ServerLevel) {
        if level.difficulty() == Difficulty::Peaceful && !self.get_type().data().allowed_in_peaceful
        {
            self.discard_in_level(level);
        } else {
            self.mob.living.no_action_time = 0;
        }
    }

    fn add_effect(
        &mut self,
        _level: &mut ServerLevel,
        _effect: MobEffectInstance,
        _source: Option<EntityId>,
    ) -> bool {
        false
    }

    fn can_ride(&self, _vehicle: &dyn Entity) -> bool {
        false
    }

    fn can_use_portal(&self, _ignore_passenger: bool) -> bool {
        false
    }

    fn can_be_affected(&self, level: &ServerLevel, effect: &MobEffectInstance) -> bool {
        if effect.effect == MobEffect::WITHER {
            false
        } else {
            crate::world::entity::living_entity::effects::can_be_affected(self, level, effect)
        }
    }
}

/// `WitherBoss.WitherDoNothingGoal`: nothing moves, jumps or looks while the wither charges up.
struct WitherDoNothingGoal;

impl Goal for WitherDoNothingGoal {
    fn name(&self) -> &'static str {
        "WitherBoss$WitherDoNothingGoal"
    }

    fn can_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        mob.as_any()
            .downcast_ref::<WitherBoss>()
            .is_some_and(|w| w.get_invulnerable_ticks() > 0)
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Jump, Flag::Look])
    }
}

/// The scenario dump fields of a wither: `inv tgt heads nhu idle dbt bossp`.
#[linkme::distributed_slice(SCENARIO_DUMP_EXTRA)]
static WITHER_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 25,
    applies: |e| e.as_any().is::<WitherBoss>(),
    fields: |e, _level, _ctx| {
        let w = e.as_any().downcast_ref::<WitherBoss>().expect("applies");
        let f = java_float_to_string;
        vec![
            ("inv".into(), w.get_invulnerable_ticks().to_string()),
            (
                "tgt".into(),
                format!(
                    "{},{},{}",
                    w.get_alternative_target(0),
                    w.get_alternative_target(1),
                    w.get_alternative_target(2)
                ),
            ),
            (
                "heads".into(),
                format!(
                    "{}/{},{}/{}",
                    f(w.x_rot_heads[0]),
                    f(w.y_rot_heads[0]),
                    f(w.x_rot_heads[1]),
                    f(w.y_rot_heads[1])
                ),
            ),
            (
                "nhu".into(),
                format!("{},{}", w.next_head_update[0], w.next_head_update[1]),
            ),
            (
                "idle".into(),
                format!("{},{}", w.idle_head_updates[0], w.idle_head_updates[1]),
            ),
            ("dbt".into(), w.destroy_blocks_tick.to_string()),
            ("bossp".into(), f(w.boss_event.get_progress())),
        ]
    },
};

#[linkme::distributed_slice(ENTITY_FACTORIES)]
static WITHER: EntityRegistration = EntityRegistration {
    entity_type: EntityType::WITHER,
    create: |ty, id, registries| Box::new(WitherBoss::new(ty, id, registries)),
};
