//! `Phantom`: the flying night mob (circle / swoop attack phases, size scaled dimensions and damage).

use crate::define_accessor;
use crate::registry::RegistryAccess;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::ai::control::body_rotation_control::{BodyRotationControl, BodyRotationControlBase};
use crate::world::entity::ai::control::look_control::{LookControl, LookControlBase};
use crate::world::entity::ai::control::move_control::{MoveControl, MoveControlBase};
use crate::world::entity::ai::goal::{Flag, Goal, GoalFlags, reduced_tick_delay};
use crate::world::entity::ai::navigation::StandardPathNavigation;
use crate::world::entity::ai::targeting::{TargetingConditions, default_conditions};
use crate::world::entity::dimensions::EntityDimensions;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::living_entity::util::{l, lm};
use crate::world::entity::mob::{self, MobData, MobKind};
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use crate::world::level::BlockGetter;
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{Attribute, EntityType, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::SoundSource;
use mc_world::heightmap::HeightmapType;

define_accessor!(pub ID_SIZE: Int = Phantom);

pub const TICKS_PER_FLAP: i32 = 17;

/// `Phantom.AttackPhase`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AttackPhase {
    Circle,
    Swoop,
}

/// `Phantom`.
pub struct Phantom {
    pub mob: MobData,
    /// `moveTargetPoint`.
    pub move_target_point: Vec3,
    /// `anchorPoint`.
    pub anchor_point: Option<BlockPos>,
    /// `attackPhase`.
    pub attack_phase: AttackPhase,
}

impl Phantom {
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mut mob = MobData::new(ty, id, MobKind::Mob, |b| {
            b.define(ID_SIZE, 0);
        });
        mob.enemy = true;
        mob.xp_reward = 5;
        mob.move_control = Some(Box::new(PhantomMoveControl { base: MoveControlBase::default(), speed: 0.1 }));
        mob.look_control = Some(Box::new(PhantomLookControl { base: LookControlBase::default() }));
        mob.body_rotation_control = Some(Box::new(PhantomBodyRotationControl { base: BodyRotationControlBase::default() }));
        let mut phantom = Phantom { mob, move_target_point: Vec3::ZERO, anchor_point: None, attack_phase: AttackPhase::Circle };
        let navigation = StandardPathNavigation::ground(&phantom);
        phantom.mob.set_navigation(Box::new(navigation));
        phantom.register_goals();
        phantom
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        self.mob.goal_selector.add_goal(1, Box::new(PhantomAttackStrategyGoal { next_sweep_tick: 0 }));
        self.mob.goal_selector.add_goal(2, Box::new(PhantomSweepAttackGoal { is_scared_of_cat: false, cat_search_tick: 0 }));
        self.mob.goal_selector.add_goal(3, Box::new(PhantomCircleAroundAnchorGoal { angle: 0.0, distance: 0.0, height: 0.0, clockwise: 0.0 }));
        self.mob.target_selector.add_goal(1, Box::new(PhantomAttackPlayerTargetGoal { attack_targeting: TargetingConditions::for_combat().range(64.0), next_scan_tick: reduced_tick_delay(20) }));
    }

    /// `setPhantomSize(size)`.
    pub fn set_phantom_size(&mut self, size: i32) {
        if self.mob.living.entity.entity_data.set(ID_SIZE, mth::clamp_i32(size, 0, 64)) {
            self.on_synced_data_updated(ID_SIZE.id());
        }
    }

    /// `getPhantomSize()`.
    pub fn get_phantom_size(&self) -> i32 {
        self.mob.living.entity.entity_data.get(ID_SIZE)
    }

    /// `updatePhantomSizeInfo()`.
    fn update_phantom_size_info(&mut self) {
        self.refresh_dimensions();
        let size = self.get_phantom_size();
        if let Some(a) = lm(self).attributes.get_instance(Attribute::ATTACK_DAMAGE) {
            a.set_base_value(f64::from(6 + size));
        }
    }

    /// `getUniqueFlapTickOffset()`.
    pub fn get_unique_flap_tick_offset(&self) -> i32 {
        self.id() * 3
    }

    /// `isFlapping()`.
    pub fn is_flapping(&self) -> bool {
        (self.get_unique_flap_tick_offset() + self.base().tick_count) % TICKS_PER_FLAP == 0
    }
}

impl Entity for Phantom {
    crate::mob_entity_accessors!(mob);

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        let size = self.get_phantom_size();
        EntityDimensions::of_type(self.mob.living.entity.entity_type).scale(1.0 + 0.15 * size as f32)
    }

    fn on_synced_data_updated(&mut self, accessor_id: u8) {
        if accessor_id == ID_SIZE.id() {
            self.update_phantom_size_info();
        }
        if accessor_id == crate::world::entity::entity::DATA_POSE.id() {
            self.refresh_dimensions();
        }
    }

    fn check_fall_damage(&mut self, _level: &mut ServerLevel, _ya: f64, _on_ground: bool, _on_state: mc_data::block::BlockState, _pos: BlockPos) {}

    fn on_climbable(&self, _level: &ServerLevel) -> bool {
        false
    }

    fn travel(&mut self, level: &mut ServerLevel, input: Vec3) {
        crate::world::entity::living_entity::travel::travel_flying(self, level, input, 0.2);
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        self.anchor_point = Some(self.block_position().above_n(5));
        self.set_phantom_size(0);
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn get_sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_PHANTOM_AMBIENT)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_PHANTOM_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_PHANTOM_DEATH)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        if let Some(pos) = self.anchor_point {
            out.put_int_array("anchor_pos", vec![pos.x(), pos.y(), pos.z()]);
        }
        out.put_int("size", self.get_phantom_size());
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.anchor_point = input.get_int_array("anchor_pos").filter(|a| a.len() == 3).map(|a| BlockPos::new(a[0], a[1], a[2]));
        self.set_phantom_size(input.get_int_or("size", 0));
    }
}

/// `canAttack(level, target, conditions)`.
fn can_attack(phantom: &dyn Entity, level: &ServerLevel, target: &dyn Entity, conditions: &TargetingConditions) -> bool {
    conditions.test(level, Some(phantom), target)
}

fn phantom_mut(mob: &mut dyn Entity) -> &mut Phantom {
    mob.as_any_mut().downcast_mut::<Phantom>().expect("phantom")
}

fn phantom(mob: &dyn Entity) -> &Phantom {
    mob.as_any().downcast_ref::<Phantom>().expect("phantom")
}

/// `PhantomAttackPlayerTargetGoal`.
struct PhantomAttackPlayerTargetGoal {
    attack_targeting: TargetingConditions,
    next_scan_tick: i32,
}

impl Goal for PhantomAttackPlayerTargetGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if self.next_scan_tick > 0 {
            self.next_scan_tick -= 1;
            return false;
        }
        self.next_scan_tick = reduced_tick_delay(60);
        let bb = mob.base().bounding_box().inflate(16.0, 64.0, 16.0);
        // `level.getNearbyPlayers(attackTargeting, this, bb)`: the players of the level in list order inside the box
        let mut players: Vec<(EntityId, f64)> = Vec::new();
        for &player_id in level.players() {
            let Some(player) = level.get_entity(player_id) else { continue };
            let b = player.base();
            if bb.contains(b.x(), b.y(), b.z()) && self.attack_targeting.test(level, Some(&*mob), player) {
                players.push((player_id, b.y()));
            }
        }
        if players.is_empty() {
            return false;
        }
        // `players.sort(comparing(Entity::getY).reversed())` (stable)
        players.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (player_id, _) in players {
            let attackable = level.get_entity(player_id).is_some_and(|p| can_attack(&*mob, level, p, &default_conditions()));
            if attackable {
                mob.set_target(level, Some(player_id));
                return true;
            }
        }
        false
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        match mob::tick::get_target_checked(mob, level) {
            Some(target) => level.get_entity(target).is_some_and(|t| can_attack(&*mob, level, t, &default_conditions())),
            None => false,
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}

/// `PhantomAttackStrategyGoal`.
struct PhantomAttackStrategyGoal {
    next_sweep_tick: i32,
}

impl PhantomAttackStrategyGoal {
    fn set_anchor_above_target(mob: &mut dyn Entity, level: &mut ServerLevel) {
        if phantom(mob).anchor_point.is_some() {
            let target_pos = mob::tick::get_target_checked(mob, level).and_then(|t| level.get_entity(t)).map(|t| t.block_position());
            let Some(target_pos) = target_pos else { return };
            let above = 20 + mob.base_mut().random.next_int_bounded(20);
            let mut anchor = target_pos.above_n(above);
            if anchor.y() < level.get_sea_level_value() {
                anchor = BlockPos::new(anchor.x(), level.get_sea_level_value() + 1, anchor.z());
            }
            phantom_mut(mob).anchor_point = Some(anchor);
        }
    }
}

impl Goal for PhantomAttackStrategyGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        match mob::tick::get_target_checked(mob, level) {
            Some(target) => level.get_entity(target).is_some_and(|t| can_attack(&*mob, level, t, &default_conditions())),
            None => false,
        }
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.next_sweep_tick = self.adjusted_tick_delay(10);
        phantom_mut(mob).attack_phase = AttackPhase::Circle;
        Self::set_anchor_above_target(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if let Some(anchor) = phantom(mob).anchor_point {
            let top = level.get_heightmap_pos(HeightmapType::MotionBlocking, anchor);
            let above = 10 + mob.base_mut().random.next_int_bounded(20);
            phantom_mut(mob).anchor_point = Some(top.above_n(above));
        }
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if phantom(mob).attack_phase == AttackPhase::Circle {
            self.next_sweep_tick -= 1;
            if self.next_sweep_tick <= 0 {
                phantom_mut(mob).attack_phase = AttackPhase::Swoop;
                Self::set_anchor_above_target(mob, level);
                self.next_sweep_tick = self.adjusted_tick_delay((8 + mob.base_mut().random.next_int_bounded(4)) * 20);
                let pitch = 0.95 + mob.base_mut().random.next_float() * 0.1;
                mob.play_sound(level, SoundEvent::ENTITY_PHANTOM_SWOOP, 10.0, pitch);
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}

/// `touchingTarget()` of `PhantomMoveTargetGoal`.
fn touching_target(mob: &dyn Entity) -> bool {
    phantom(mob).move_target_point.distance_to_sqr(mob.base().x(), mob.base().y(), mob.base().z()) < 4.0
}

/// `PhantomCircleAroundAnchorGoal`.
struct PhantomCircleAroundAnchorGoal {
    angle: f32,
    distance: f32,
    height: f32,
    clockwise: f32,
}

impl PhantomCircleAroundAnchorGoal {
    fn select_next(&mut self, mob: &mut dyn Entity) {
        if phantom(mob).anchor_point.is_none() {
            let pos = mob.block_position();
            phantom_mut(mob).anchor_point = Some(pos);
        }
        self.angle += self.clockwise * 15.0 * (std::f64::consts::PI / 180.0) as f32;
        let anchor = phantom(mob).anchor_point.expect("anchor");
        let lower_corner = Vec3::at_lower_corner_of(&anchor);
        let d = f64::from(self.distance);
        phantom_mut(mob).move_target_point = lower_corner.add(d * f64::from(mth::cos(f64::from(self.angle))), f64::from(-4.0 + self.height), d * f64::from(mth::sin(f64::from(self.angle))));
    }
}

impl Goal for PhantomCircleAroundAnchorGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        mob::tick::get_target_checked(mob, level).is_none() || phantom(mob).attack_phase == AttackPhase::Circle
    }

    fn start(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        let random = &mut mob.base_mut().random;
        self.distance = 5.0 + random.next_float() * 10.0;
        self.height = -4.0 + random.next_float() * 9.0;
        self.clockwise = if random.next_boolean() { 1.0 } else { -1.0 };
        self.select_next(mob);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let bound = self.adjusted_tick_delay(350);
        if mob.base_mut().random.next_int_bounded(bound) == 0 {
            self.height = -4.0 + mob.base_mut().random.next_float() * 9.0;
        }
        let bound = self.adjusted_tick_delay(250);
        if mob.base_mut().random.next_int_bounded(bound) == 0 {
            self.distance += 1.0;
            if self.distance > 15.0 {
                self.distance = 5.0;
                self.clockwise = -self.clockwise;
            }
        }
        let bound = self.adjusted_tick_delay(450);
        if mob.base_mut().random.next_int_bounded(bound) == 0 {
            self.angle = mob.base_mut().random.next_float() * 2.0 * std::f32::consts::PI;
            self.select_next(mob);
        }
        if touching_target(mob) {
            self.select_next(mob);
        }
        if phantom(mob).move_target_point.y < mob.base().y() && !level.is_empty_block(mob.block_position().below_n(1)) {
            self.height = self.height.max(1.0);
            self.select_next(mob);
        }
        if phantom(mob).move_target_point.y > mob.base().y() && !level.is_empty_block(mob.block_position().above_n(1)) {
            self.height = self.height.min(-1.0);
            self.select_next(mob);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

/// `PhantomSweepAttackGoal`.
struct PhantomSweepAttackGoal {
    is_scared_of_cat: bool,
    cat_search_tick: i32,
}

impl Goal for PhantomSweepAttackGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        mob::tick::get_target_checked(mob, level).is_some() && phantom(mob).attack_phase == AttackPhase::Swoop
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let Some(target) = mob::tick::get_target_checked(mob, level) else { return false };
        let Some(target_entity) = level.get_entity(target) else { return false };
        if !target_entity.is_alive() {
            return false;
        }
        if target_entity.is_player() && (target_entity.is_spectator() || mob::ext::is_creative_player(target_entity)) {
            return false;
        }
        if !self.can_use(mob, level) {
            return false;
        }
        if mob.base().tick_count > self.cat_search_tick {
            self.cat_search_tick = mob.base().tick_count + 20;
            let bb = mob.base().bounding_box().inflate(16.0, 16.0, 16.0);
            let cats = level.get_entities_of_class(&bb, |e| e.get_type() == EntityType::CAT, |e| e.is_alive());
            for &cat in &cats {
                level.with_entity(cat, |level, cat| cat.hiss(level));
            }
            self.is_scared_of_cat = !cats.is_empty();
        }
        !self.is_scared_of_cat
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        mob.set_target(level, None);
        phantom_mut(mob).attack_phase = AttackPhase::Circle;
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(target) = mob::tick::get_target_checked(mob, level) else { return };
        let Some((tx, ty, tz, target_bb)) = level.get_entity(target).map(|t| (t.base().x(), t.base().get_y(0.5), t.base().z(), t.base().bounding_box())) else { return };
        phantom_mut(mob).move_target_point = Vec3::new(tx, ty, tz);
        if mob.base().bounding_box().inflate(f64::from(0.2f32), f64::from(0.2f32), f64::from(0.2f32)).intersects(&target_bb) {
            let hit_target = level.with_entity(target, |level, t| mob.do_hurt_target(level, t));
            let _ = hit_target;
            phantom_mut(mob).attack_phase = AttackPhase::Circle;
            if !mob.base().is_silent() {
                level.level_event_impl(None, 1039, mob.block_position(), 0);
            }
        } else if mob.base().horizontal_collision || l(mob).hurt_time > 0 {
            phantom_mut(mob).attack_phase = AttackPhase::Circle;
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

/// `PhantomLookControl`: does nothing.
struct PhantomLookControl {
    base: LookControlBase,
}

impl LookControl for PhantomLookControl {
    fn base(&self) -> &LookControlBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut LookControlBase {
        &mut self.base
    }

    fn tick(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {}
}

/// `PhantomBodyRotationControl`.
struct PhantomBodyRotationControl {
    base: BodyRotationControlBase,
}

impl BodyRotationControl for PhantomBodyRotationControl {
    fn base(&self) -> &BodyRotationControlBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BodyRotationControlBase {
        &mut self.base
    }

    fn client_tick(&mut self, mob: &mut dyn Entity) {
        if let Some(living) = mob.living_mut() {
            living.y_head_rot = living.y_body_rot;
            living.y_body_rot = mob.base().y_rot();
        }
    }
}

/// `PhantomMoveControl`.
struct PhantomMoveControl {
    base: MoveControlBase,
    speed: f32,
}

impl MoveControl for PhantomMoveControl {
    fn base(&self) -> &MoveControlBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut MoveControlBase {
        &mut self.base
    }

    fn tick(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        if mob.base().horizontal_collision {
            let y_rot = mob.base().y_rot() + 180.0;
            mob.set_y_rot(y_rot);
            self.speed = 0.1;
        }
        let target = phantom(mob).move_target_point;
        let mut tdx = target.x - mob.base().x();
        let mut tdy = target.y - mob.base().y();
        let mut tdz = target.z - mob.base().z();
        let mut sd = (tdx * tdx + tdz * tdz).sqrt();
        if sd.abs() > f64::from(1.0E-5f32) {
            let y_relative_scale = 1.0 - (tdy * f64::from(0.7f32)).abs() / sd;
            tdx *= y_relative_scale;
            tdz *= y_relative_scale;
            sd = (tdx * tdx + tdz * tdz).sqrt();
            let sd2 = (tdx * tdx + tdz * tdz + tdy * tdy).sqrt();
            let prev = mob.base().y_rot();
            let angle = mth::atan2(tdz, tdx) as f32;
            let a = mth::wrap_degrees_f32(mob.base().y_rot() + 90.0);
            let b = mth::wrap_degrees_f32(angle * (180.0f32 / std::f32::consts::PI));
            let new_y_rot = mth::approach_degrees(a, b, 4.0) - 90.0;
            mob.set_y_rot(new_y_rot);
            let y_rot = mob.base().y_rot();
            if let Some(living) = mob.living_mut() {
                living.y_body_rot = y_rot;
            }
            if mth::degrees_difference_abs(prev, y_rot) < 3.0 {
                self.speed = mth::approach(self.speed, 1.8, 0.005 * (1.8 / self.speed));
            } else {
                self.speed = mth::approach(self.speed, 0.2, 0.025);
            }
            let x_rot_d = (-(mth::atan2(-tdy, sd) * f64::from(180.0f32) / f64::from(std::f32::consts::PI))) as f32;
            mob.set_x_rot(x_rot_d);
            let move_angle = mob.base().y_rot() + 90.0;
            let deg_to_rad = (std::f64::consts::PI / 180.0) as f32;
            let txd = f64::from(self.speed * mth::cos(f64::from(move_angle * deg_to_rad))) * (tdx / sd2).abs();
            let tzd = f64::from(self.speed * mth::sin(f64::from(move_angle * deg_to_rad))) * (tdz / sd2).abs();
            let tyd = f64::from(self.speed * mth::sin(f64::from(x_rot_d * deg_to_rad))) * (tdy / sd2).abs();
            let movement = mob.base().delta_movement();
            let new_movement = movement.add_vec(Vec3::new(txd, tyd, tzd).subtract(movement).scale(0.2));
            mob.base_mut().set_delta_movement(new_movement);
        }
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static PHANTOM: EntityRegistration = EntityRegistration { entity_type: EntityType::PHANTOM, create: |ty, id, registries| Box::new(Phantom::new(ty, id, registries)) };
