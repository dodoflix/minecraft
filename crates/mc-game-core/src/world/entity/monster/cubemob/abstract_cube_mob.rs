//! `AbstractCubeMob` (26.3: an `AgeableMob`): size, squish animation, jump/move control, the cube goals, splitting on death,
//! touch damage. The concrete classes (`Slime`, `MagmaCube`, `SulfurCube`) embed [`CubeMobData`] next to their `MobData`,
//! implement [`CubeMob`] (the virtual methods of the class) and delegate their `Entity` overrides to the free functions
//! of this module (the `super` calls).

use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::ai::control::move_control::{MoveControl, MoveControlBase, Operation};
use crate::world::entity::ai::goal::{Flag, Goal, GoalFlags, reduced_tick_delay};
use crate::world::entity::dimensions::EntityDimensions;
use crate::world::entity::living_entity::util::{l, lm};
use crate::world::entity::mob::{self, ConversionParams, ConversionType};
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::entity::{Entity, EntityId, RemovalReason, ServerPlayer};
use crate::world::item::enchantment::actors::Actors;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{Attribute, EntityType, ParticleType, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::SoundSource;

define_accessor!(pub ID_SIZE: Int = AbstractCubeMob);

pub const MIN_SIZE: i32 = 1;
pub const MAX_SIZE: i32 = 127;
pub const MAX_NATURAL_SIZE: i32 = 4;

/// The fields `AbstractCubeMob` adds to `AgeableMob`.
#[derive(Default, Debug)]
pub struct CubeMobData {
    pub target_squish: f32,
    pub squish: f32,
    pub o_squish: f32,
    pub was_on_ground: bool,
    /// `doWaterSplashEffect()` decided by `onSyncedDataUpdated(ID_SIZE)` (which has no level at hand); played by the
    /// next `set_size` with a level or the next tick.
    pub pending_water_splash: bool,
}

/// The virtual surface of `AbstractCubeMob` (abstract and overridable methods) plus its data.
pub trait CubeMob: Entity {
    fn cube(&self) -> &CubeMobData;
    fn cube_mut(&mut self) -> &mut CubeMobData;

    /// `getParticleType()`.
    fn get_particle_type(&self) -> Option<ParticleType>;
    /// `getSquishSound()`.
    fn get_squish_sound(&self) -> SoundEvent;
    /// `getJumpSound()`.
    fn get_jump_sound(&self) -> SoundEvent;

    /// `getSize()`.
    fn get_size(&self) -> i32 {
        self.base().entity_data.get(ID_SIZE)
    }
    /// `isTiny()`.
    fn is_tiny(&self) -> bool {
        self.get_size() <= 1
    }
    /// `getJumpDelay()`.
    fn get_jump_delay(&mut self) -> i32 {
        self.base_mut().random.next_int_bounded(20) + 10
    }
    /// `decreaseSquish()`.
    fn decrease_squish(&mut self) {
        self.cube_mut().target_squish *= 0.6;
    }
    /// `canDealDamage()`.
    fn can_deal_damage(&self) -> bool {
        !self.is_tiny() && self.is_effective_ai()
    }
    /// `getAttackDamage()`.
    fn get_attack_damage(&self) -> f32 {
        l(self.as_dyn()).get_attribute_value(Attribute::ATTACK_DAMAGE) as f32
    }
    /// `getSplitCount()`.
    fn get_split_count(&mut self) -> i32 {
        2 + self.base_mut().random.next_int_bounded(3)
    }
    /// `doPlayJumpSound()`.
    fn do_play_jump_sound(&self) -> bool {
        self.get_size() > 0
    }
    /// `getSoundPitch()`.
    fn get_sound_pitch(&mut self) -> f32 {
        let pitch_adjuster = if self.is_tiny() { 1.4f32 } else { 0.8f32 };
        let random = &mut self.base_mut().random;
        ((random.next_float() - random.next_float()) * 0.2 + 1.0) * pitch_adjuster
    }
    /// `setCubeMobHealth(actualSize)`.
    fn set_cube_mob_health(&mut self, actual_size: i32) {
        if let Some(health) = lm(self.as_dyn_mut())
            .attributes
            .get_instance(Attribute::MAX_HEALTH)
        {
            health.set_base_value(f64::from(actual_size.wrapping_mul(actual_size)));
        }
    }
    /// `setSize(size, updateHealth)`.
    fn set_size(&mut self, size: i32, update_health: bool) {
        set_size(self, size, update_health);
    }
    /// `setSpawnSize(level, difficulty)`.
    fn set_spawn_size(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
    ) {
        let random = level.random();
        let mut size_scale = random.next_int_bounded(3);
        if size_scale < 2 && random.next_float() < 0.5 * difficulty.get_special_multiplier() {
            size_scale += 1;
        }
        self.set_size(1 << size_scale, true);
    }
    /// What `setUpSplitCube` of this class does to the new cube besides `super` (`SulfurCube`: `setBaby(true)`).
    fn split_cubes_are_babies(&self) -> bool {
        false
    }
    /// Whether the `CubeMobMoveControl` runs (`SulfurCubeMobMoveControl`: not while a body item is swallowed).
    fn cube_move_control_active(&self) -> bool {
        true
    }
}

/// The `AbstractCubeMob` view of `e`.
pub fn cube(e: &dyn Entity) -> &dyn CubeMob {
    e.as_cube_mob().expect("cube mob")
}

pub fn cube_mut(e: &mut dyn Entity) -> &mut dyn CubeMob {
    e.as_cube_mob_mut().expect("cube mob")
}

// ---- CubeMobMoveControl ----------------------------------------------------------------------------------------------

/// `AbstractCubeMob.CubeMobMoveControl`.
pub struct CubeMobMoveControl {
    base: MoveControlBase,
    y_rot: f32,
    jump_delay: i32,
    is_aggressive: bool,
}

impl CubeMobMoveControl {
    /// `new CubeMobMoveControl(cubeMob)` (`yRot = 180.0F * cubeMob.getYRot() / (float)Math.PI`).
    pub fn new(cube_mob_y_rot: f32) -> Self {
        Self {
            base: MoveControlBase::default(),
            y_rot: 180.0 * cube_mob_y_rot / std::f32::consts::PI,
            jump_delay: 0,
            is_aggressive: false,
        }
    }

    /// `setDirection(yRot, isAggressive)`.
    pub fn set_direction(&mut self, y_rot: f32, is_aggressive: bool) {
        self.y_rot = y_rot;
        self.is_aggressive = is_aggressive;
    }

    /// `setWantedMovement(speedModifier)`.
    pub fn set_wanted_movement(&mut self, speed_modifier: f64) {
        self.base.speed_modifier = speed_modifier;
        self.base.operation = Operation::MoveTo;
    }
}

impl MoveControl for CubeMobMoveControl {
    fn base(&self) -> &MoveControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut MoveControlBase {
        &mut self.base
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if !cube(mob).cube_move_control_active() {
            return;
        }
        let y_rot = self.base.rotlerp(mob.base().y_rot(), self.y_rot, 90.0);
        mob.set_y_rot(y_rot);
        lm(mob).y_head_rot = y_rot;
        lm(mob).y_body_rot = y_rot;
        if self.base.operation != Operation::MoveTo {
            mob::set_zza(mob, 0.0);
        } else {
            self.base.operation = Operation::Wait;
            let speed = (self.base.speed_modifier
                * mob::attribute_value(mob, Attribute::MOVEMENT_SPEED))
                as f32;
            if mob.base().on_ground() {
                mob.set_speed(speed);
                let delay = self.jump_delay;
                self.jump_delay -= 1;
                if delay <= 0 {
                    self.jump_delay = cube_mut(mob).get_jump_delay();
                    if self.is_aggressive {
                        self.jump_delay /= 3;
                    }
                    mob::jump_control_jump(mob);
                    if cube(mob).do_play_jump_sound() {
                        let sound = cube(mob).get_jump_sound();
                        let volume = mob.get_sound_volume();
                        let pitch = cube_mut(mob).get_sound_pitch();
                        mob.play_sound(level, sound, volume, pitch);
                    }
                } else {
                    let living = lm(mob);
                    living.xxa = 0.0;
                    living.zza = 0.0;
                    mob.set_speed(0.0);
                }
            } else {
                mob.set_speed(speed);
            }
        }
    }
}

/// `getMoveControl() instanceof CubeMobMoveControl` with the control at hand.
fn with_cube_move_control<R>(
    mob: &mut dyn Entity,
    f: impl FnOnce(&mut CubeMobMoveControl) -> R,
) -> Option<R> {
    let control = mob.mob_mut()?.move_control.as_deref_mut()?;
    (control as &mut dyn std::any::Any)
        .downcast_mut::<CubeMobMoveControl>()
        .map(f)
}

/// `getMoveControl() instanceof CubeMobMoveControl`.
fn has_cube_move_control(mob: &dyn Entity) -> bool {
    mob.mob()
        .and_then(|m| m.move_control.as_deref())
        .is_some_and(|c| (c as &dyn std::any::Any).is::<CubeMobMoveControl>())
}

/// `cubeMobMoveControl.setDirection(yRot, isAggressive)` when the mob has one.
pub fn set_direction(mob: &mut dyn Entity, y_rot: f32, is_aggressive: bool) {
    with_cube_move_control(mob, |c| c.set_direction(y_rot, is_aggressive));
}

/// `cubeMobMoveControl.setWantedMovement(speedModifier)` when the mob has one.
pub fn set_wanted_movement(mob: &mut dyn Entity, speed_modifier: f64) {
    with_cube_move_control(mob, |c| c.set_wanted_movement(speed_modifier));
}

/// `addTargetingGoals()` of `Slime` / `MagmaCube`.
pub fn add_slime_targeting_goals(selector: &mut crate::world::entity::ai::goal::GoalSelector) {
    use crate::world::entity::ai::goal::target::{NearestAttackableTargetGoal, TargetType};
    selector.add_goal(
        1,
        Box::new(
            NearestAttackableTargetGoal::build(TargetType::Player, 10, true, false, None)
                .with_source_selector(std::sync::Arc::new(
                    |mob: &dyn Entity, target: &dyn Entity, _level: &ServerLevel| {
                        (target.base().y() - mob.base().y()).abs() <= 4.0
                    },
                )),
        ),
    );
    selector.add_goal(
        3,
        Box::new(NearestAttackableTargetGoal::new(
            TargetType::of_type(EntityType::IRON_GOLEM),
            true,
        )),
    );
}

// ---- goals -----------------------------------------------------------------------------------------------------------

/// `AbstractCubeMob.CubeMobAttackGoal`.
#[derive(Default)]
pub struct CubeMobAttackGoal {
    grow_tired_timer: i32,
}

impl CubeMobAttackGoal {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Goal for CubeMobAttackGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let Some(target) =
            mob::tick::get_target_checked(mob, level).and_then(|t| level.get_entity(t))
        else {
            return false;
        };
        mob.can_attack(level, target) && has_cube_move_control(mob)
    }

    fn start(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.grow_tired_timer = reduced_tick_delay(300);
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let Some(target) =
            mob::tick::get_target_checked(mob, level).and_then(|t| level.get_entity(t))
        else {
            return false;
        };
        if !mob.can_attack(level, target) {
            return false;
        }
        self.grow_tired_timer -= 1;
        self.grow_tired_timer > 0
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if let Some(target) = mob::tick::get_target_checked(mob, level) {
            if let Some(target) = level.get_entity(target) {
                mob::ext::mob_look_at(mob, target, 10.0, 10.0);
            }
        }
        if has_cube_move_control(mob) {
            let can_deal_damage = cube(mob).can_deal_damage();
            let y_rot = mob.base().y_rot();
            set_direction(mob, y_rot, can_deal_damage);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Look])
    }
}

/// `AbstractCubeMob.CubeMobFloatGoal`.
pub struct CubeMobFloatGoal;

impl CubeMobFloatGoal {
    /// `CubeMobFloatGoal(mob)`: `mob.getNavigation().setCanFloat(true)`.
    pub fn new(mob: &mut dyn Entity) -> Self {
        mob::navigation::set_can_float(mob, true);
        Self
    }
}

impl Goal for CubeMobFloatGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        mob.is_in_liquid() && has_cube_move_control(mob)
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        if mob.base_mut().random.next_float() < 0.8 {
            mob::jump_control_jump(mob);
        }
        set_wanted_movement(mob, 1.2);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Jump, Flag::Move])
    }
}

/// `AbstractCubeMob.CubeMobKeepOnJumpingGoal`.
pub struct CubeMobKeepOnJumpingGoal;

impl Goal for CubeMobKeepOnJumpingGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        !mob.base().is_passenger()
    }

    fn tick(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        set_wanted_movement(mob, 1.0);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Jump, Flag::Move])
    }
}

/// `AbstractCubeMob.CubeMobRandomDirectionGoal`.
#[derive(Default)]
pub struct CubeMobRandomDirectionGoal {
    chosen_degrees: f32,
    next_randomize_time: i32,
}

impl CubeMobRandomDirectionGoal {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Goal for CubeMobRandomDirectionGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        mob::tick::get_target_checked(mob, level).is_none()
            && (mob.base().on_ground()
                || mob.is_in_liquid()
                || mob.has_effect(mc_data::MobEffect::LEVITATION))
            && has_cube_move_control(mob)
    }

    fn tick(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.next_randomize_time -= 1;
        if self.next_randomize_time <= 0 {
            let random = &mut mob.base_mut().random;
            let delay = 40 + random.next_int_bounded(60);
            self.next_randomize_time = self.adjusted_tick_delay(delay);
            self.chosen_degrees = mob.base_mut().random.next_int_bounded(360) as f32;
        }
        set_direction(mob, self.chosen_degrees, false);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Look])
    }
}

// ---- Entity overrides (the bodies; concrete classes delegate) -----------------------------------------------------------

/// `getDefaultDimensions(pose)`: `getType().getDimensions().scale(getSize())`, then `LivingEntity.getDimensions`' `scale(getScale())`.
pub fn get_dimensions<T: CubeMob + ?Sized>(this: &T, pose: Pose) -> EntityDimensions {
    let default = EntityDimensions::of_type(this.base().entity_type).scale(this.get_size() as f32);
    mob::ext::living_dimensions(this.as_dyn(), pose, default)
}

/// `refreshDimensions()`: `super.refreshDimensions()` then `setPos(oldX, oldY, oldZ)`; the position is never moved by the
/// (deferred) `fudgePositionAfterSizeChange`, so the override only has to leave it alone.
pub fn refresh_dimensions<T: CubeMob + ?Sized>(this: &mut T) {
    let pose = this.base().get_pose();
    let new_dim = this.get_dimensions(pose);
    let base = this.base_mut();
    base.eye_height = new_dim.eye_height;
    base.dimensions = new_dim;
    this.reapply_position();
}

/// `onSyncedDataUpdated(accessor)`.
pub fn on_synced_data_updated<T: CubeMob + ?Sized>(this: &mut T, accessor_id: u8) {
    if accessor_id == ID_SIZE.id() {
        this.refresh_dimensions();
        let y_head_rot = l(this.as_dyn()).y_head_rot;
        this.set_y_rot(y_head_rot);
        lm(this.as_dyn_mut()).y_body_rot = y_head_rot;
        if this.is_in_water() && this.base_mut().random.next_int_bounded(20) == 0 {
            this.cube_mut().pending_water_splash = true;
        }
    }
    // `super.onSyncedDataUpdated(accessor)`: `Entity` (pose) and `AgeableMob` (baby flag)
    if accessor_id == crate::world::entity::entity::DATA_POSE.id() {
        this.refresh_dimensions();
    }
    if accessor_id == mob::DATA_BABY_ID.id() {
        this.refresh_dimensions();
    }
}

/// `setSize(size, updateHealth)`.
pub fn set_size<T: CubeMob + ?Sized>(this: &mut T, size: i32, update_health: bool) {
    let actual_size = mth::clamp_i32(size, 1, 127);
    if this.base_mut().entity_data.set(ID_SIZE, actual_size) {
        this.on_synced_data_updated(ID_SIZE.id());
    }
    this.reapply_position();
    this.refresh_dimensions();
    this.set_cube_mob_health(actual_size);
    if let Some(speed) = lm(this.as_dyn_mut())
        .attributes
        .get_instance(Attribute::MOVEMENT_SPEED)
    {
        speed.set_base_value(f64::from(0.2f32 + 0.1f32 * actual_size as f32));
    }
    if update_health {
        let living = lm(this.as_dyn_mut());
        let max_health = living.get_max_health();
        living.set_health(max_health);
    }
}

/// `setSize` for a caller with the level at hand: plays the splash `onSyncedDataUpdated(ID_SIZE)` decided.
pub fn set_size_in_level<T: CubeMob + ?Sized>(
    this: &mut T,
    level: &mut ServerLevel,
    size: i32,
    update_health: bool,
) {
    this.set_size(size, update_health);
    play_pending_splash(this, level);
}

fn play_pending_splash<T: CubeMob + ?Sized>(this: &mut T, level: &mut ServerLevel) {
    if std::mem::take(&mut this.cube_mut().pending_water_splash) {
        this.do_water_splash_effect(level);
    }
}

/// `tick()`.
pub fn tick<T: CubeMob + ?Sized>(this: &mut T, level: &mut ServerLevel) {
    play_pending_splash(this, level);
    let data = this.cube_mut();
    data.o_squish = data.squish;
    data.squish += (data.target_squish - data.squish) * 0.5;
    mob::tick::tick(this.as_dyn_mut(), level);
    let on_ground = this.base().on_ground();
    if on_ground && !this.cube().was_on_ground {
        let pose = this.base().get_pose();
        let size = this.get_dimensions(pose).width * 2.0;
        let _radius = size / 2.0;
        // `level().addParticle(...)` is a no-op on the server; its random draws are not
        let mut i = 0;
        while (i as f32) < size * 16.0 {
            let random = &mut this.base_mut().random;
            random.next_float();
            random.next_float();
            i += 1;
        }
        let sound = this.get_squish_sound();
        let volume = this.get_sound_volume();
        let pitch = {
            let random = &mut this.base_mut().random;
            ((random.next_float() - random.next_float()) * 0.2 + 1.0) / 0.8
        };
        this.play_sound(level, sound, volume, pitch);
        this.cube_mut().target_squish = -0.5;
    } else if !on_ground && this.cube().was_on_ground {
        this.cube_mut().target_squish = 1.0;
    }
    this.cube_mut().was_on_ground = on_ground;
    this.decrease_squish();
}

/// `remove(reason)`, the part before `super.remove(reason)`: a dead cube mob larger than 1 splits.
pub fn before_remove<T: CubeMob + ?Sized>(
    this: &mut T,
    level: &mut ServerLevel,
    _reason: RemovalReason,
) {
    let size = this.get_size();
    if size <= 1 || !this.is_dead_or_dying() {
        return;
    }
    let pose = this.base().get_pose();
    let width = this.get_dimensions(pose).width;
    let xz_cube_spawn_offset = width / 2.0;
    let half_size = size / 2;
    let count = this.get_split_count();
    let team = level
        .scoreboard()
        .get_players_team(&this.get_scoreboard_name())
        .map(|t| t.get_name().to_string());
    let params = ConversionParams {
        ty: ConversionType::SplitOnDeath,
        keep_equipment: false,
        preserve_can_pick_up_loot: false,
        team,
    };
    let ty = this.get_type();
    let babies = this.split_cubes_are_babies();
    for i in 0..count {
        let xd = ((i % 2) as f32 - 0.5) * xz_cube_spawn_offset;
        let zd = ((i / 2) as f32 - 0.5) * xz_cube_spawn_offset;
        // `setUpSplitCube`: the rotation draw of `this.random` is the only draw `convertTo` leaves between the conversion
        // and the `afterConversion` callback, so it is made first
        let yaw = this.base_mut().random.next_float() * 360.0;
        let pos = this.base().position();
        mob::convert::convert_to(
            this.as_dyn_mut(),
            level,
            ty,
            &params,
            EntitySpawnReason::Triggered,
            |cube_mob, _level| {
                cube_mut(cube_mob).set_size(half_size, true);
                cube_mob.snap_to(
                    pos.x + f64::from(xd),
                    pos.y + 0.5,
                    pos.z + f64::from(zd),
                    yaw,
                    0.0,
                );
                if babies {
                    mob::ageable::set_baby(cube_mob, true);
                }
            },
        );
    }
}

/// `push(entity)`.
pub fn push_entity<T: CubeMob + ?Sized>(
    this: &mut T,
    level: &mut ServerLevel,
    other: &mut dyn Entity,
) {
    crate::world::entity::entity::push_entity_base(this, level, other);
    if other.get_type() == EntityType::IRON_GOLEM && this.can_deal_damage() {
        deal_damage(this, level, other);
    }
}

/// `playerTouch(player)`.
pub fn player_touch<T: CubeMob + ?Sized>(
    this: &mut T,
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
) {
    if this.can_deal_damage() {
        deal_damage(this, level, player);
    }
}

/// `dealDamage(target)`.
pub fn deal_damage<T: CubeMob + ?Sized>(
    this: &mut T,
    level: &mut ServerLevel,
    target: &mut dyn Entity,
) {
    if this.is_alive()
        && crate::world::entity::team::do_teams_allow_damage(level, this.as_dyn(), target)
        && mob::ext::is_within_melee_attack_range(level, this.as_dyn(), target)
        && mob::ext::has_line_of_sight(level, this.as_dyn(), target)
    {
        let damage_source: DamageSource = level.damage_sources().mob_attack(this.as_dyn());
        let damage = this.get_attack_damage();
        if target.hurt_server(level, &damage_source, damage) {
            let pitch = {
                let random = &mut this.base_mut().random;
                (random.next_float() - random.next_float()) * 0.2 + 1.0
            };
            this.play_sound(level, SoundEvent::ENTITY_SLIME_ATTACK, 1.0, pitch);
            let victim: EntityId = target.id();
            crate::world::item::enchantment_helper::do_post_attack_effects(
                level,
                &mut Actors::of(target).and(this.as_dyn_mut()),
                victim,
                &damage_source,
            );
        }
    }
}

/// `getPassengerAttachmentPoint(passenger, dimensions, scale)`.
pub fn get_passenger_attachment_point<T: CubeMob + ?Sized>(
    this: &T,
    dimensions: &EntityDimensions,
    scale: f32,
) -> Vec3 {
    Vec3::new(
        0.0,
        f64::from(dimensions.height) - 0.015625 * f64::from(this.get_size()) * f64::from(scale),
        0.0,
    )
}

/// `jumpFromGround()`.
pub fn jump_from_ground<T: CubeMob + ?Sized>(
    this: &mut T,
    level: &ServerLevel,
    extra_jump_power: f32,
) {
    let movement = this.base().delta_movement();
    let jump_power = this.get_jump_power(level) + extra_jump_power;
    this.base_mut()
        .set_delta_movement_xyz(movement.x, f64::from(jump_power), movement.z);
    this.base_mut().needs_sync = true;
}

/// `finalizeSpawn(level, difficulty, spawnReason, groupData)`.
pub fn finalize_spawn<T: CubeMob + ?Sized>(
    this: &mut T,
    level: &mut dyn ServerLevelAccessor,
    difficulty: DifficultyInstance,
    reason: EntitySpawnReason,
    group_data: Option<Box<dyn SpawnGroupData>>,
) -> Option<Box<dyn SpawnGroupData>> {
    let data = mob::spawn::finalize_spawn(this.as_dyn_mut(), level, difficulty, reason, group_data);
    this.set_spawn_size(level, difficulty);
    data
}

/// `addAdditionalSaveData(output)`.
pub fn add_additional_save_data<T: CubeMob + ?Sized>(this: &T, out: &mut CompoundTag) {
    mob::nbt::add_additional_save_data(this.as_dyn(), out);
    out.put_int("Size", this.get_size() - 1);
    out.put_boolean("wasOnGround", this.cube().was_on_ground);
}

/// `readAdditionalSaveData(input)`.
pub fn read_additional_save_data<T: CubeMob + ?Sized>(this: &mut T, input: &CompoundTag) {
    this.set_size(input.get_int_or("Size", 0).wrapping_add(1), false);
    mob::nbt::read_additional_save_data(this.as_dyn_mut(), input);
    this.cube_mut().was_on_ground = input.get_boolean_or("wasOnGround", false);
}

/// `getSoundSource()` of the hostile cube mobs.
pub const HOSTILE: SoundSource = SoundSource::Hostile;
