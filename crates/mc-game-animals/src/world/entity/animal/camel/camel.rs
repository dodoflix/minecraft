//! `net.minecraft.world.entity.animal.camel.Camel` and `CamelHusk`: a two-seat desert mount with a sit/stand state machine, a
//! dash with a cooldown and a brain. The camel husk is the same class with another set of sounds and food that neither breeds
//! nor grows up and carries mobs; one Rust type serves both entity types.
//!
//! Not implemented: the client-only animation states (`setupAnimationStates`, `updateWalkAnimation`) and the quad leash
//! offsets, which only the client reads.

use super::camel_ai;
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{Attribute, EntityType, GameEvent, Item, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::brain::{Brain, Provider};
use mc_game_core::world::entity::ai::control::body_rotation_control::{
    BodyRotationControl, BodyRotationControlBase, base_client_tick,
};
use mc_game_core::world::entity::ai::control::look_control::default_tick;
use mc_game_core::world::entity::ai::control::move_control::base_tick;
use mc_game_core::world::entity::ai::control::{
    LookControl, LookControlBase, MoveControl, MoveControlBase, Operation,
};
use mc_game_core::world::entity::ai::memory::BrainPacked;
use mc_game_core::world::entity::ai::navigation::{PathNavigation as _, StandardPathNavigation};
use mc_game_core::world::entity::ai::sensing::sensor_type::{
    FOOD_TEMPTATIONS, HURT_BY, NEAREST_ADULT, NEAREST_LIVING_ENTITIES,
};
use mc_game_core::world::entity::animal::equine::abstract_horse::{
    self, AbstractHorse, AbstractHorseData, DATA_ID_FLAGS,
};
use mc_game_core::world::entity::dimensions::EntityDimensions;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{
    self, ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::living_entity::{self, util::l};
use mc_game_core::world::entity::mob::leashable::is_leashed;
use mc_game_core::world::entity::mob::{self, MobData, MobKind, ageable, animal};
use mc_game_core::world::entity::movement::collision_context;
use mc_game_core::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::spawn_predicates;
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::block::state::BlockStateExt as _;
use mc_game_core::world::level::collision::CollisionGetter as _;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;

define_accessor!(pub DASH: Boolean = Camel);
define_accessor!(pub LAST_POSE_CHANGE_TICK: Long = Camel);

/// `DASH_COOLDOWN_TICKS`.
pub const DASH_COOLDOWN_TICKS: i32 = 55;
/// `MAX_HEAD_Y_ROT`.
pub const MAX_HEAD_Y_ROT: i32 = 30;
const RUNNING_SPEED_BONUS: f32 = 0.1;
const DASH_VERTICAL_MOMENTUM: f32 = 1.4285;
const DASH_HORIZONTAL_MOMENTUM: f32 = 22.2222;
const SITDOWN_DURATION_TICKS: i64 = 40;
const STANDUP_DURATION_TICKS: i64 = 52;
const SITTING_HEIGHT_DIFFERENCE: f32 = 1.43;
const BABY_SCALE: f32 = 0.6;

/// `BRAIN_PROVIDER`.
static BRAIN_PROVIDER: Provider = Provider::with_activities(
    &[
        &NEAREST_LIVING_ENTITIES,
        &HURT_BY,
        &FOOD_TEMPTATIONS,
        &NEAREST_ADULT,
    ],
    camel_ai::get_activities,
);

/// `BABY_STANDING_DIMENSIONS`.
fn baby_standing_dimensions() -> EntityDimensions {
    EntityDimensions::scalable(0.95, 1.4).with_eye_height(1.38)
}

/// `BABY_SITTING_DIMENSIONS`.
fn baby_sitting_dimensions() -> EntityDimensions {
    EntityDimensions::scalable(0.95, 0.425).with_eye_height(0.41)
}

/// `ADULT_SITTING_DIMENSIONS`.
fn adult_sitting_dimensions() -> EntityDimensions {
    let camel = EntityType::CAMEL.data();
    EntityDimensions::scalable(camel.width, camel.height - SITTING_HEIGHT_DIFFERENCE)
        .with_eye_height(0.845)
}

/// `Camel` and `CamelHusk`.
pub struct Camel {
    pub mob: MobData,
    pub horse: AbstractHorseData,
    /// `dashCooldown`.
    dash_cooldown: i32,
    /// The game time the camel last saw: the rider position, the leash offset and the body control read the pose animation
    /// where no level is at hand.
    game_time: i64,
}

impl Camel {
    /// `new Camel(type, level)` / `new CamelHusk(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_ID_FLAGS, 0i8);
            b.define(DASH, false);
            b.define(LAST_POSE_CHANGE_TICK, 0i64);
        });
        let mut camel = Camel {
            mob,
            horse: AbstractHorseData::new(),
            dash_cooldown: 0,
            game_time: 0,
        };
        // `LivingEntity.<init>`: `this.brain = this.makeBrain(Brain.Packed.EMPTY)`
        let brain = camel.make_brain(&BrainPacked::EMPTY);
        camel.mob.living.brain = brain;
        let mut navigation = StandardPathNavigation::ground(&camel);
        navigation.set_can_float(true);
        navigation.data_mut().set_can_walk_over_fences(true);
        camel.mob.set_navigation(Box::new(navigation));
        camel.create_inventory();
        camel.mob.move_control = Some(Box::<CamelMoveControl>::default());
        camel.mob.look_control = Some(Box::<CamelLookControl>::default());
        camel.mob.body_rotation_control = Some(Box::<CamelBodyRotationControl>::default());
        camel
    }

    /// `this instanceof CamelHusk`.
    pub fn is_husk(&self) -> bool {
        self.base().entity_type == EntityType::CAMEL_HUSK
    }

    /// `isMobControlled()`: only the husk is driven by the mob that rides it.
    pub fn is_mob_controlled(&self) -> bool {
        self.is_husk() && self.horse.first_passenger_is_mob
    }

    /// `isDashing()`.
    pub fn is_dashing(&self) -> bool {
        self.base().entity_data.get(DASH)
    }

    /// `setDashing(isDashing)`.
    pub fn set_dashing(&mut self, is_dashing: bool) {
        if self.base_mut().entity_data.set(DASH, is_dashing) {
            self.on_synced_data_updated(DASH.id());
        }
    }

    /// `isCamelSitting()`.
    pub fn is_camel_sitting(&self) -> bool {
        self.base().entity_data.get(LAST_POSE_CHANGE_TICK) < 0
    }

    /// `getPoseTime()` at game time `now`.
    pub fn get_pose_time(&self, now: i64) -> i64 {
        now - self.base().entity_data.get(LAST_POSE_CHANGE_TICK).abs()
    }

    /// `isInPoseTransition()` at game time `now`.
    pub fn is_in_pose_transition(&self, now: i64) -> bool {
        self.get_pose_time(now)
            < if self.is_camel_sitting() {
                SITDOWN_DURATION_TICKS
            } else {
                STANDUP_DURATION_TICKS
            }
    }

    /// `refuseToMove()` at game time `now`.
    pub fn refuse_to_move(&self, now: i64) -> bool {
        self.is_camel_sitting() || self.is_in_pose_transition(now)
    }

    /// `refuseToMove()` at the game time the camel last saw.
    fn refuses_to_move_now(&self) -> bool {
        self.refuse_to_move(self.game_time)
    }

    /// `canCamelChangePose()`: the other pose's box is free of blocks.
    pub fn can_camel_change_pose(&self, level: &ServerLevel) -> bool {
        let target = if self.is_camel_sitting() {
            Pose::Standing
        } else {
            Pose::Sitting
        };
        let bounds = self
            .get_dimensions(target)
            .make_bounding_box_vec(self.position());
        level.no_block_collision(Some(&collision_context(self)), &bounds)
    }

    /// `sitDown()`.
    pub fn sit_down(&mut self, level: &mut ServerLevel) {
        self.game_time = level.game_time();
        if !self.is_camel_sitting() {
            let sound = self.sit_down_sound();
            living_entity::damage::make_sound(self, level, sound);
            self.set_pose(Pose::Sitting);
            self.game_event(level, GameEvent::ENTITY_ACTION);
            self.reset_last_pose_change_tick(-level.game_time());
        }
    }

    /// `standUp()`.
    pub fn stand_up(&mut self, level: &mut ServerLevel) {
        self.game_time = level.game_time();
        if self.is_camel_sitting() {
            let sound = self.stand_up_sound();
            living_entity::damage::make_sound(self, level, sound);
            self.set_pose(Pose::Standing);
            self.game_event(level, GameEvent::ENTITY_ACTION);
            self.reset_last_pose_change_tick(level.game_time());
        }
    }

    /// `standUpInstantly()`.
    pub fn stand_up_instantly(&mut self, level: &mut ServerLevel) {
        self.game_time = level.game_time();
        self.set_pose(Pose::Standing);
        self.game_event(level, GameEvent::ENTITY_ACTION);
        self.reset_last_pose_change_tick_to_full_stand(level.game_time());
    }

    /// `resetLastPoseChangeTick(syncedPoseTickTime)`.
    pub fn reset_last_pose_change_tick(&mut self, synced_pose_tick_time: i64) {
        self.base_mut()
            .entity_data
            .set(LAST_POSE_CHANGE_TICK, synced_pose_tick_time);
    }

    /// `resetLastPoseChangeTickToFullStand(currentTime)`.
    fn reset_last_pose_change_tick_to_full_stand(&mut self, current_time: i64) {
        self.reset_last_pose_change_tick((current_time - 52 - 1).max(0));
    }

    /// `getJumpCooldown()`.
    pub fn get_jump_cooldown(&self) -> i32 {
        self.dash_cooldown
    }

    fn sit_down_sound(&self) -> SoundEvent {
        if self.is_husk() {
            SoundEvent::ENTITY_CAMEL_HUSK_SIT
        } else {
            SoundEvent::ENTITY_CAMEL_SIT
        }
    }

    fn stand_up_sound(&self) -> SoundEvent {
        if self.is_husk() {
            SoundEvent::ENTITY_CAMEL_HUSK_STAND
        } else {
            SoundEvent::ENTITY_CAMEL_STAND
        }
    }

    fn dashing_sound(&self) -> SoundEvent {
        if self.is_husk() {
            SoundEvent::ENTITY_CAMEL_HUSK_DASH
        } else {
            SoundEvent::ENTITY_CAMEL_DASH
        }
    }

    fn dash_ready_sound(&self) -> SoundEvent {
        if self.is_husk() {
            SoundEvent::ENTITY_CAMEL_HUSK_DASH_READY
        } else {
            SoundEvent::ENTITY_CAMEL_DASH_READY
        }
    }

    /// `getBodyAnchorAnimationYOffset(isFront, partialTicks, dimensions, scale)`: where a rider sits while the camel sits down
    /// and stands up.
    fn get_body_anchor_animation_y_offset(
        &self,
        is_front: bool,
        partial_ticks: f32,
        dimensions: &EntityDimensions,
        scale: f32,
    ) -> f64 {
        let age_sit_y_offset = if self.is_baby() { 0.09375 } else { 0.375 };
        let mut base_sit_offset = f64::from(dimensions.height) - age_sit_y_offset;
        let sitting_height_difference = scale * SITTING_HEIGHT_DIFFERENCE;
        let vertical_drop = sitting_height_difference - scale * 0.2;
        let bottom_point = sitting_height_difference - vertical_drop;
        let now = self.game_time;
        let is_in_transition = self.is_in_pose_transition(now);
        let is_sitting = self.is_camel_sitting();
        if is_in_transition {
            let animation_duration: i32 = if is_sitting { 40 } else { 52 };
            let (half_point, flex_point_offset): (i32, f32) = if is_sitting {
                (28, if is_front { 0.5 } else { 0.1 })
            } else {
                (
                    if is_front { 24 } else { 32 },
                    if is_front { 0.6 } else { 0.35 },
                )
            };
            let pose_time = mth::clamp_f32(
                self.get_pose_time(now) as f32 + partial_ticks,
                0.0,
                animation_duration as f32,
            );
            let is_first_part = pose_time < half_point as f32;
            let part = if is_first_part {
                pose_time / half_point as f32
            } else {
                (pose_time - half_point as f32) / (animation_duration - half_point) as f32
            };
            let flex_point = sitting_height_difference - flex_point_offset * vertical_drop;
            base_sit_offset += f64::from(if is_sitting {
                mth::lerp_f32(
                    part,
                    if is_first_part {
                        sitting_height_difference
                    } else {
                        flex_point
                    },
                    if is_first_part {
                        flex_point
                    } else {
                        bottom_point
                    },
                )
            } else {
                mth::lerp_f32(
                    part,
                    if is_first_part {
                        bottom_point - sitting_height_difference
                    } else {
                        bottom_point - flex_point
                    },
                    if is_first_part {
                        bottom_point - flex_point
                    } else {
                        0.0
                    },
                )
            });
        }
        if is_sitting && !is_in_transition {
            base_sit_offset += f64::from(bottom_point);
        }
        base_sit_offset
    }
}

impl AbstractHorse for Camel {
    fn horse(&self) -> &AbstractHorseData {
        &self.horse
    }
    fn horse_mut(&mut self) -> &mut AbstractHorseData {
        &mut self.horse
    }
    fn get_eating_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_husk() {
            SoundEvent::ENTITY_CAMEL_HUSK_EAT
        } else {
            SoundEvent::ENTITY_CAMEL_EAT
        })
    }
    fn can_perform_rearing(&self) -> bool {
        false
    }
    fn is_mob_controlled(&self) -> bool {
        Camel::is_mob_controlled(self)
    }
    fn always_tamed(&self) -> bool {
        true
    }

    fn handle_eating(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        stack: &ItemStack,
    ) -> bool {
        if !self.is_food(stack) {
            return false;
        }
        let could_heal = self.get_health() < l(self).get_max_health();
        if could_heal {
            self.heal(level, 2.0);
        }
        let could_set_in_love = abstract_horse::is_tamed(self)
            && ageable::get_age(self) == 0
            && self.can_fall_in_love();
        if could_set_in_love {
            animal::set_in_love(self, level, Some(&*player));
        }
        let could_age_up = self.can_age_up();
        if could_age_up {
            // the particle call's arguments draw from the entity random although the server shows no particle
            let base = self.base_mut();
            base.get_random_x(1.0);
            base.get_random_y();
            base.get_random_z(1.0);
            ageable::age_up(self, level, 10, false);
        }
        if !could_heal && !could_set_in_love && !could_age_up {
            return false;
        }
        if !self.base().is_silent()
            && let Some(sound) = self.get_eating_sound()
        {
            let random = &mut self.base_mut().random;
            let pitch = 1.0 + (random.next_float() - random.next_float()) * 0.2;
            let (position, source) = (self.position(), self.get_sound_source());
            level.play_sound_at(
                None, position.x, position.y, position.z, sound, source, 1.0, pitch,
            );
        }
        self.game_event(level, GameEvent::EAT);
        true
    }

    fn horse_tick_post(&mut self, level: &mut ServerLevel) {
        self.game_time = level.game_time();
        if self.is_dashing()
            && self.dash_cooldown < 50
            && (self.base().on_ground() || self.is_in_liquid() || self.base().is_passenger())
        {
            self.set_dashing(false);
        }
        if self.dash_cooldown > 0 {
            self.dash_cooldown -= 1;
            if self.dash_cooldown == 0 {
                let pos = self.block_position();
                let sound = self.dash_ready_sound();
                level.play_sound_at(
                    None,
                    f64::from(pos.x()) + 0.5,
                    f64::from(pos.y()) + 0.5,
                    f64::from(pos.z()) + 0.5,
                    sound,
                    mc_protocol::packets::play::SoundSource::Neutral,
                    1.0,
                    1.0,
                );
            }
        }
        if self.refuse_to_move(level.game_time()) {
            mob::tick::clamp_head_rotation_to_body(self);
        }
        if self.is_camel_sitting() && self.is_in_water() {
            self.stand_up_instantly(level);
        }
    }

    fn horse_ridden_rotation(
        &self,
        level: &ServerLevel,
        controller_x_rot: f32,
        controller_y_rot: f32,
    ) -> (f32, f32) {
        if self.refuse_to_move(level.game_time()) {
            (self.base().x_rot(), self.base().y_rot())
        } else {
            (controller_x_rot * 0.5, controller_y_rot)
        }
    }

    fn horse_ridden_input(
        &self,
        level: &ServerLevel,
        controller: EntityId,
        self_input: Vec3,
    ) -> Vec3 {
        if self.refuse_to_move(level.game_time()) {
            Vec3::ZERO
        } else {
            abstract_horse::get_ridden_input(self, level, controller, self_input)
        }
    }

    fn horse_ridden_speed(&self, level: &ServerLevel, controller: EntityId) -> f32 {
        let sprinting = level
            .get_entity(controller)
            .is_some_and(|c| c.is_sprinting());
        let movement_bonus = if sprinting && self.get_jump_cooldown() == 0 {
            RUNNING_SPEED_BONUS
        } else {
            0.0
        };
        l(self).get_attribute_value(Attribute::MOVEMENT_SPEED) as f32 + movement_bonus
    }

    fn horse_tick_ridden_post(&mut self, level: &mut ServerLevel, controller: EntityId) {
        let moving_forward = level
            .get_entity(controller)
            .and_then(|c| c.living())
            .is_some_and(|c| c.zza > 0.0);
        if moving_forward
            && self.is_camel_sitting()
            && !self.is_in_pose_transition(level.game_time())
        {
            self.stand_up(level);
        }
    }

    fn horse_can_jump(&self, level: &ServerLevel) -> bool {
        !self.refuse_to_move(level.game_time()) && abstract_horse::is_saddled(self)
    }

    fn horse_on_player_jump(&mut self, jump_amount: i32) {
        if abstract_horse::is_saddled(self) && self.dash_cooldown <= 0 && self.base().on_ground() {
            abstract_horse::on_player_jump(self, jump_amount);
        }
    }

    fn horse_execute_riders_jump(&mut self, level: &mut ServerLevel, amount: f32, _input: Vec3) {
        let jump_momentum = f64::from(self.get_jump_power(level));
        let speed = l(self).get_attribute_value(Attribute::MOVEMENT_SPEED);
        let block_speed_factor =
            f64::from(self.modify_block_speed_factor(self.get_block_speed_factor(level)));
        let horizontal = self
            .get_look_angle()
            .multiply_xyz(1.0, 0.0, 1.0)
            .normalize()
            .scale(f64::from(DASH_HORIZONTAL_MOMENTUM * amount) * speed * block_speed_factor);
        let momentum = horizontal.add(Vec3::new(
            0.0,
            f64::from(DASH_VERTICAL_MOMENTUM * amount) * jump_momentum,
            0.0,
        ));
        self.base_mut().add_delta_movement(momentum);
        self.dash_cooldown = DASH_COOLDOWN_TICKS;
        self.set_dashing(true);
        self.base_mut().needs_sync = true;
    }

    fn horse_handle_start_jump(&mut self, level: &mut ServerLevel, _jump_scale: i32) {
        let sound = self.dashing_sound();
        living_entity::damage::make_sound(self, level, sound);
        self.game_event(level, GameEvent::ENTITY_ACTION);
        self.set_dashing(true);
    }

    fn horse_open_custom_inventory_screen(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
    ) {
        let id = self.id();
        level.lend(self, |level| player.open_horse_inventory(level, id));
    }

    fn get_saddle_sound(&self) -> SoundEvent {
        if self.is_husk() {
            SoundEvent::ENTITY_CAMEL_HUSK_SADDLE
        } else {
            SoundEvent::ENTITY_CAMEL_SADDLE
        }
    }

    fn horse_on_elastic_leash_pull_post(&mut self, level: &mut ServerLevel) {
        if self.is_camel_sitting()
            && !self.is_in_pose_transition(level.game_time())
            && self.can_camel_change_pose(level)
        {
            self.stand_up(level);
        }
    }
}

impl Entity for Camel {
    mc_game_core::mob_entity_accessors!(mob);
    mc_game_core::abstract_horse_entity!();

    fn get_encode_id(&self) -> Option<&'static str> {
        Some(if self.is_husk() {
            "minecraft:camel_husk"
        } else {
            "minecraft:camel"
        })
    }

    fn make_brain(&mut self, packed: &BrainPacked) -> Brain {
        BRAIN_PROVIDER.make_brain(self, packed)
    }

    fn get_dimensions(&self, pose: Pose) -> EntityDimensions {
        let default_dimensions = if pose == Pose::Sitting {
            if self.is_baby() {
                baby_sitting_dimensions()
            } else {
                adult_sitting_dimensions()
            }
        } else if self.is_baby() {
            baby_standing_dimensions()
        } else {
            EntityDimensions::of_type(self.base().entity_type)
        };
        mob::ext::living_dimensions(self, pose, default_dimensions)
    }

    fn on_synced_data_updated(&mut self, accessor_id: u8) {
        if !self.base().first_tick && accessor_id == DASH.id() {
            self.dash_cooldown = if self.dash_cooldown == 0 {
                DASH_COOLDOWN_TICKS
            } else {
                self.dash_cooldown
            };
        }
        if accessor_id == mc_game_core::world::entity::entity::DATA_POSE.id()
            || accessor_id == mob::DATA_BABY_ID.id()
        {
            self.refresh_dimensions();
        }
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        self.game_time = level.game_time();
        Brain::tick(level, self);
        camel_ai::update_activity(self);
        animal::custom_server_ai_step(self);
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        abstract_horse::ai_step(self, level);
    }

    fn travel(&mut self, level: &mut ServerLevel, mut input: Vec3) {
        if self.refuse_to_move(level.game_time()) && self.base().on_ground() {
            let movement = self.base().delta_movement();
            self.base_mut()
                .set_delta_movement(movement.multiply_xyz(0.0, 1.0, 0.0));
            input = input.multiply_xyz(0.0, 1.0, 0.0);
        }
        living_entity::travel::travel(self, level, input);
    }

    fn drop_equipment(&mut self, level: &mut ServerLevel) {
        abstract_horse::drop_equipment(self, level);
    }

    fn can_use_slot(&self, slot: EquipmentSlot) -> bool {
        abstract_horse::can_use_slot(self, slot)
    }

    fn is_immobile(&self) -> bool {
        abstract_horse::is_immobile(self)
    }

    fn cause_fall_damage(
        &mut self,
        level: &mut ServerLevel,
        fall_distance: f64,
        damage_modifier: f32,
        source: &DamageSource,
    ) -> bool {
        abstract_horse::cause_fall_damage(self, level, fall_distance, damage_modifier, source)
    }

    fn actually_hurt(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) {
        self.stand_up_instantly(level);
        animal::reset_love(self);
        living_entity::damage::actually_hurt(self, level, source, damage);
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_husk() {
            SoundEvent::ENTITY_CAMEL_HUSK_AMBIENT
        } else {
            SoundEvent::ENTITY_CAMEL_AMBIENT
        })
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_husk() {
            SoundEvent::ENTITY_CAMEL_HUSK_DEATH
        } else {
            SoundEvent::ENTITY_CAMEL_DEATH
        })
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(if self.is_husk() {
            SoundEvent::ENTITY_CAMEL_HUSK_HURT
        } else {
            SoundEvent::ENTITY_CAMEL_HURT
        })
    }

    fn play_step_sound(
        &mut self,
        level: &mut ServerLevel,
        _pos: BlockPos,
        state: mc_data::block::BlockState,
    ) {
        let sand = state.block().is_in_tag("camel_sand_step_sound_blocks");
        let (sound, volume) = match (self.is_husk(), sand) {
            (false, true) => (SoundEvent::ENTITY_CAMEL_STEP_SAND, 1.0),
            (false, false) => (SoundEvent::ENTITY_CAMEL_STEP, 1.0),
            (true, true) => (SoundEvent::ENTITY_CAMEL_HUSK_STEP_SAND, 0.4),
            (true, false) => (SoundEvent::ENTITY_CAMEL_HUSK_STEP, 0.4),
        };
        self.play_sound(level, sound, volume, 1.0);
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        let tag = if self.is_husk() {
            "camel_husk_food"
        } else {
            "camel_food"
        };
        mc_data::tags::get::<Item>(tag).is_some_and(|t| t.contains(stack.item()))
    }

    fn interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
        location: Vec3,
    ) -> InteractionResult {
        if self.is_husk() {
            self.mob.persistence_required = true;
        }
        mob::interact::interact(self, level, player, hand, location)
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        if player.is_shift_key_down() && !self.is_baby() {
            self.horse_open_custom_inventory_screen(level, player);
            return InteractionResult::SUCCESS;
        }
        let item_stack = player.get_item_in_hand(hand);
        let result = mob::interact::with_hand_stack(player, hand, |stack, player| {
            mc_game_core::world::item::interact_living_entity(level, stack, player, self, hand)
        });
        if result.consumes_action() {
            result
        } else if self.is_food(&item_stack) {
            abstract_horse::fed_food(self, level, player, hand)
        } else if self.base().passengers.len() < 2 && !self.is_baby() {
            self.do_player_ride(level, player);
            InteractionResult::CONSUME
        } else if self.is_baby() && abstract_horse::is_holding(player, Item::GOLDEN_DANDELION) {
            abstract_horse::mob_interact(self, level, player, hand)
        } else {
            InteractionResult::FAIL
        }
    }

    fn can_mate(&self, partner: &dyn Entity) -> bool {
        !self.is_husk()
            && partner.id() != self.id()
            && partner.as_any().is::<Camel>()
            && abstract_horse::can_parent(self)
            && partner.as_abstract_horse().is_some_and(|h| h.can_parent())
    }

    fn can_fall_in_love(&self) -> bool {
        !self.is_husk() && animal::can_fall_in_love(self)
    }

    fn can_be_a_baby(&self) -> bool {
        !self.is_husk()
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        _partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        if self.is_husk() {
            return None;
        }
        factory::create(
            EntityType::CAMEL,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )
    }

    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        self.is_husk()
    }

    fn can_be_leashed(&self) -> bool {
        !self.is_husk() || !self.is_mob_controlled()
    }

    fn charge_speed_modifier(&self) -> f32 {
        if self.is_husk() { 4.0 } else { 1.0 }
    }

    fn get_age_scale(&self) -> f32 {
        if self.is_baby() { BABY_SCALE } else { 1.0 }
    }

    fn get_max_head_y_rot(&self) -> i32 {
        MAX_HEAD_Y_ROT
    }

    fn can_add_passenger(&self, _passenger: &dyn Entity) -> bool {
        self.base().passengers.len() <= 2
    }

    fn get_passenger_riding_position(&self, passenger: &dyn Entity) -> Vec3 {
        let scale = mob::ext::get_scale_times_age_scale(self);
        self.position().add(self.get_passenger_attachment_point(
            passenger,
            &self.base().dimensions,
            scale,
        ))
    }

    fn get_passenger_attachment_point(
        &self,
        passenger: &dyn Entity,
        dimensions: &EntityDimensions,
        scale: f32,
    ) -> Vec3 {
        let index = self
            .base()
            .passengers
            .iter()
            .position(|&p| p == passenger.id())
            .unwrap_or(0);
        let driver = index == 0;
        let mut offset = 0.5f32;
        let height = if self.base().is_removed() {
            0.01
        } else {
            self.get_body_anchor_animation_y_offset(driver, 0.0, dimensions, scale) as f32
        };
        if self.base().passengers.len() > 1 {
            if !driver {
                offset = -0.7;
            }
            if passenger.mob().is_some_and(|m| m.animal.is_some()) {
                offset += 0.2;
            }
        }
        Vec3::new(0.0, f64::from(height), f64::from(offset * scale))
            .y_rot(-self.base().y_rot() * (std::f64::consts::PI / 180.0) as f32)
    }

    fn get_leash_offset(&self) -> Vec3 {
        let scale = self.get_age_scale();
        let dimensions = self.get_dimensions(self.base().get_pose());
        Vec3::new(
            0.0,
            self.get_body_anchor_animation_y_offset(true, 0.0, &dimensions, scale)
                - f64::from(0.2 * scale),
            f64::from(dimensions.width * 0.56),
        )
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        camel_ai::init_memories(self, level.random());
        self.game_time = level.get_game_time();
        self.reset_last_pose_change_tick_to_full_stand(self.game_time);
        let group_data = abstract_horse::finalize_spawn_prepare(self, level, group_data);
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        abstract_horse::add_additional_save_data(self, out);
        out.put_long(
            "LastPoseTick",
            self.base().entity_data.get(LAST_POSE_CHANGE_TICK),
        );
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        abstract_horse::read_additional_save_data(self, level, input);
        let pose_tick = input.get_long_or("LastPoseTick", 0);
        if pose_tick < 0 {
            self.set_pose(Pose::Sitting);
        }
        self.reset_last_pose_change_tick(pose_tick);
    }
}

/// `Camel.checkCamelSpawnRules`.
pub fn check_camel_spawn_rules(
    _ty: EntityType,
    level: &dyn ServerLevelAccessor,
    _reason: EntitySpawnReason,
    pos: BlockPos,
    _random: &mut dyn RandomSource,
) -> bool {
    level
        .get_block_state(pos.below())
        .is_tag("camels_spawnable_on")
        && spawn_predicates::is_bright_enough_to_spawn(level, pos)
}

mc_game_core::register_spawn_placement!(EntityType::CAMEL, check_camel_spawn_rules);

/// `Camel.CamelMoveControl`: a sitting camel that is told to walk stands up first.
#[derive(Default)]
struct CamelMoveControl {
    base: MoveControlBase,
}

impl MoveControl for CamelMoveControl {
    fn base(&self) -> &MoveControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut MoveControlBase {
        &mut self.base
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if self.base.operation == Operation::MoveTo
            && !is_leashed(&*mob)
            && let Some(camel) = mob.as_any_mut().downcast_mut::<Camel>()
            && camel.is_camel_sitting()
            && !camel.is_in_pose_transition(level.game_time())
            && camel.can_camel_change_pose(level)
        {
            camel.stand_up(level);
        }
        base_tick(&mut self.base, mob, level);
    }
}

/// `Camel.CamelLookControl`: the rider steers the head.
#[derive(Default)]
struct CamelLookControl {
    base: LookControlBase,
}

impl LookControl for CamelLookControl {
    fn base(&self) -> &LookControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut LookControlBase {
        &mut self.base
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if !mob.has_controlling_passenger() {
            default_tick(self, mob, level);
        }
    }
}

/// `Camel.CamelBodyRotationControl`: the body stays put while the camel refuses to move.
#[derive(Default)]
struct CamelBodyRotationControl {
    base: BodyRotationControlBase,
}

impl BodyRotationControl for CamelBodyRotationControl {
    fn base(&self) -> &BodyRotationControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BodyRotationControlBase {
        &mut self.base
    }
    fn client_tick(&mut self, mob: &mut dyn Entity) {
        let refuses = mob
            .as_any()
            .downcast_ref::<Camel>()
            .is_some_and(Camel::refuses_to_move_now);
        if !refuses {
            base_client_tick(&mut self.base, mob);
        }
    }
}

/// The scenario dump fields of a camel: `dash lastpose dashcd`.
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static CAMEL_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 31,
    applies: |e| e.as_any().is::<Camel>(),
    fields: |e, _level, _ctx| {
        let camel = e.as_any().downcast_ref::<Camel>().expect("applies");
        vec![
            ("dash".into(), u8::from(camel.is_dashing()).to_string()),
            (
                "lastpose".into(),
                camel
                    .base()
                    .entity_data
                    .get(LAST_POSE_CHANGE_TICK)
                    .to_string(),
            ),
            ("dashcd".into(), camel.dash_cooldown.to_string()),
        ]
    },
};

#[distributed_slice(ENTITY_FACTORIES)]
static CAMEL: EntityRegistration = EntityRegistration {
    entity_type: EntityType::CAMEL,
    create: |ty, id, _registries| Box::new(Camel::new(ty, id)),
};

#[distributed_slice(ENTITY_FACTORIES)]
static CAMEL_HUSK: EntityRegistration = EntityRegistration {
    entity_type: EntityType::CAMEL_HUSK,
    create: |ty, id, _registries| Box::new(Camel::new(ty, id)),
};
