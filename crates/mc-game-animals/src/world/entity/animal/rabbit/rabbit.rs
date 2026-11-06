//! `Rabbit`: hopping movement (`RabbitJumpControl`, `RabbitMoveControl`), variants incl. the killer bunny, carrot raiding.

use super::rabbit_goals::{RabbitAvoidEntityGoal, RabbitPanicGoal, RaidGardenGoal};
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{Attribute, EntityType, Item, SoundEvent};
use mc_game_ai::world::entity::ai::goal::ClimbOnTopOfPowderSnowGoal;
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::registry::keys;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::control::{
    JumpControl, JumpControlBase, MoveControl, MoveControlBase, Operation, move_control,
};
use mc_game_core::world::entity::ai::goal::target::{
    HurtByTargetGoal, NearestAttackableTargetGoal, TargetType,
};
use mc_game_core::world::entity::ai::goal::{
    BreedGoal, FloatGoal, LookAtPlayerGoal, MeleeAttackGoal, TemptGoal,
    WaterAvoidingRandomStrollGoal,
};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::attributes::{AttributeModifier, AttributeModifierOperation};
use mc_game_core::world::entity::dimensions::EntityDimensions;
use mc_game_core::world::entity::factory::{
    ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::living_entity::jump;
use mc_game_core::world::entity::mob::spawn::AgeableMobGroupData;
use mc_game_core::world::entity::mob::{self, MobData, MobKind, navigation};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::spawn_predicates;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::block::BlockStateExt as _;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;
use mc_protocol::components::{DataComponentMap, RABBIT_VARIANT, RabbitVariant};
use mc_protocol::packets::play::SoundSource;
use mc_text::Component;
use std::sync::{Arc, LazyLock};

define_accessor!(pub DATA_TYPE_ID: Int = Rabbit);

pub const STROLL_SPEED_MOD: f64 = 0.6;
pub const BREED_SPEED_MOD: f64 = 0.8;
pub const FOLLOW_SPEED_MOD: f64 = 1.0;
pub const FLEE_SPEED_MOD: f64 = 2.2;
pub const ATTACK_SPEED_MOD: f64 = 1.4;

static EVIL_ATTACK_POWER_MODIFIER: LazyLock<Identifier> =
    LazyLock::new(|| Identifier::new("minecraft", "evil").expect("valid id"));

/// `Rabbit.Variant.DEFAULT`.
pub const DEFAULT_VARIANT: RabbitVariant = RabbitVariant::Brown;

/// `Rabbit.Variant.byId(id)` (`ByIdMap.sparse(.., DEFAULT)`).
pub fn variant_by_id(id: i32) -> RabbitVariant {
    RabbitVariant::ALL
        .iter()
        .copied()
        .find(|v| v.wire_id() == id)
        .unwrap_or(DEFAULT_VARIANT)
}

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    EntityDimensions::scalable(0.24, 0.4).with_eye_height(0.39)
}

/// `Rabbit.RabbitGroupData`.
pub struct RabbitGroupData {
    base: AgeableMobGroupData,
    pub variant: RabbitVariant,
}

impl RabbitGroupData {
    fn new(variant: RabbitVariant) -> Self {
        Self {
            base: AgeableMobGroupData::with_chance(1.0),
            variant,
        }
    }
}

impl SpawnGroupData for RabbitGroupData {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn as_ageable_mut(&mut self) -> Option<&mut AgeableMobGroupData> {
        Some(&mut self.base)
    }
}

/// `Rabbit.RabbitJumpControl`.
#[derive(Default)]
pub struct RabbitJumpControl {
    base: JumpControlBase,
    can_jump: bool,
}

impl RabbitJumpControl {
    pub fn want_jump(&self) -> bool {
        self.base.jump
    }

    pub fn can_jump(&self) -> bool {
        self.can_jump
    }

    pub fn set_can_jump(&mut self, can_jump: bool) {
        self.can_jump = can_jump;
    }
}

impl JumpControl for RabbitJumpControl {
    fn base(&self) -> &JumpControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut JumpControlBase {
        &mut self.base
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if self.base.jump {
            if let Some(rabbit) = mob.as_any_mut().downcast_mut::<Rabbit>() {
                rabbit.start_jumping(level);
            }
            self.base.jump = false;
        }
    }
}

/// `Rabbit.RabbitMoveControl`.
#[derive(Default)]
pub struct RabbitMoveControl {
    base: MoveControlBase,
    next_jump_speed: f64,
}

impl MoveControl for RabbitMoveControl {
    fn base(&self) -> &MoveControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut MoveControlBase {
        &mut self.base
    }

    fn set_wanted_position_for(
        &mut self,
        mob: &dyn Entity,
        x: f64,
        y: f64,
        z: f64,
        mut speed_modifier: f64,
    ) {
        if mob.is_in_water() {
            speed_modifier = 1.5;
        }
        self.set_wanted_position(x, y, z, speed_modifier);
        if speed_modifier > 0.0 {
            self.next_jump_speed = speed_modifier;
        }
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let want_jump = mob
            .mob()
            .and_then(|m| m.jump_control.as_deref())
            .and_then(|c| (c as &dyn std::any::Any).downcast_ref::<RabbitJumpControl>())
            .is_some_and(RabbitJumpControl::want_jump);
        let jumping = mob.living().is_some_and(|l| l.jumping);
        if mob.base().on_ground() && !jumping && !want_jump {
            self.set_speed_modifier(mob, level, 0.0);
        } else if self.has_wanted() || self.base.operation == Operation::Jumping {
            let speed = self.next_jump_speed;
            self.set_speed_modifier(mob, level, speed);
        }
        move_control::base_tick(&mut self.base, mob, level);
    }
}

impl RabbitMoveControl {
    /// `Rabbit.setSpeedModifier(speed)` while this control ticks (it is taken out of the mob).
    fn set_speed_modifier(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel, speed: f64) {
        navigation::set_speed_modifier(mob, level, speed);
        let (x, y, z) = (self.base.wanted_x, self.base.wanted_y, self.base.wanted_z);
        self.set_wanted_position_for(&*mob, x, y, z, speed);
    }
}

/// `Rabbit`.
pub struct Rabbit {
    pub mob: MobData,
    jump_ticks: i32,
    jump_duration: i32,
    was_on_ground: bool,
    jump_delay_ticks: i32,
    pub more_carrot_ticks: i32,
    /// Client animation timer; the field initializer still draws from the entity RNG.
    idle_animation_timeout: i32,
}

/// `ItemTags.RABBIT_FOOD`.
fn is_rabbit_food(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("rabbit_food").is_some_and(|t| t.contains(stack.item()))
}

impl Rabbit {
    /// `new Rabbit(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_TYPE_ID, DEFAULT_VARIANT.wire_id());
        });
        let mut rabbit = Rabbit {
            mob,
            jump_ticks: 0,
            jump_duration: 0,
            was_on_ground: false,
            jump_delay_ticks: 0,
            more_carrot_ticks: 0,
            idle_animation_timeout: 0,
        };
        let navigation = StandardPathNavigation::ground(&rabbit);
        rabbit.mob.set_navigation(Box::new(navigation));
        rabbit.register_goals();
        rabbit.idle_animation_timeout = rabbit.mob.living.entity.random.next_int_bounded(40) + 180;
        rabbit.mob.jump_control = Some(Box::<RabbitJumpControl>::default());
        rabbit.mob.move_control = Some(Box::<RabbitMoveControl>::default());
        rabbit.set_speed_modifier_ctor(0.0);
        rabbit
    }

    /// `setSpeedModifier(speed)` in the constructor (no level yet: `navigation.setSpeedModifier` only stores the value).
    fn set_speed_modifier_ctor(&mut self, speed: f64) {
        if let Some(n) = self.mob.navigation.as_deref_mut() {
            n.set_speed_modifier(speed);
        }
        let (x, y, z) = {
            let c = self.mob.move_control.as_deref().expect("move control");
            (c.get_wanted_x(), c.get_wanted_y(), c.get_wanted_z())
        };
        mob::set_wanted_position(self, x, y, z, speed);
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let float = FloatGoal::new(self);
        let breed = BreedGoal::new(self, BREED_SPEED_MOD);
        let look_at = LookAtPlayerGoal::new_player(self, 10.0);
        let climb = ClimbOnTopOfPowderSnowGoal::new();
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(1, Box::new(float));
        goals.add_goal(1, Box::new(climb));
        goals.add_goal(1, Box::new(RabbitPanicGoal::new(FLEE_SPEED_MOD)));
        goals.add_goal(2, Box::new(breed));
        goals.add_goal(
            3,
            Box::new(TemptGoal::new(
                1.0,
                Arc::new(|stack: &ItemStack| is_rabbit_food(stack)),
                false,
            )),
        );
        goals.add_goal(
            4,
            Box::new(RabbitAvoidEntityGoal::new(
                Arc::new(|e: &dyn Entity| e.is_player()),
                8.0,
                FLEE_SPEED_MOD,
                FLEE_SPEED_MOD,
            )),
        );
        goals.add_goal(
            4,
            Box::new(RabbitAvoidEntityGoal::new(
                Arc::new(|e: &dyn Entity| e.get_type() == EntityType::WOLF),
                10.0,
                FLEE_SPEED_MOD,
                FLEE_SPEED_MOD,
            )),
        );
        goals.add_goal(
            4,
            Box::new(RabbitAvoidEntityGoal::new(
                Arc::new(|e: &dyn Entity| e.mob().is_some_and(|m| m.kind == MobKind::Monster)),
                4.0,
                FLEE_SPEED_MOD,
                FLEE_SPEED_MOD,
            )),
        );
        goals.add_goal(5, Box::new(RaidGardenGoal::new()));
        goals.add_goal(
            6,
            Box::new(WaterAvoidingRandomStrollGoal::new(STROLL_SPEED_MOD)),
        );
        goals.add_goal(11, Box::new(look_at));
    }

    /// `getVariant()`.
    pub fn get_variant(&self) -> RabbitVariant {
        variant_by_id(self.mob.living.entity.entity_data.get(DATA_TYPE_ID))
    }

    /// `setVariant(variant)`.
    pub fn set_variant(&mut self, variant: RabbitVariant) {
        if variant == RabbitVariant::Evil {
            if let Some(armor) = self.mob.living.attributes.get_instance(Attribute::ARMOR) {
                armor.set_base_value(8.0);
            }
            self.mob
                .goal_selector
                .add_goal(4, Box::new(MeleeAttackGoal::new(ATTACK_SPEED_MOD, true)));
            self.mob.target_selector.add_goal(
                1,
                Box::new(HurtByTargetGoal::new(Vec::new()).set_alert_others(Vec::new())),
            );
            self.mob.target_selector.add_goal(
                2,
                Box::new(NearestAttackableTargetGoal::new(TargetType::Player, true)),
            );
            self.mob.target_selector.add_goal(
                2,
                Box::new(NearestAttackableTargetGoal::new(
                    TargetType::of_type(EntityType::WOLF),
                    true,
                )),
            );
            if let Some(attack) = self
                .mob
                .living
                .attributes
                .get_instance(Attribute::ATTACK_DAMAGE)
            {
                attack.add_or_update_transient_modifier(AttributeModifier {
                    id: EVIL_ATTACK_POWER_MODIFIER.clone(),
                    amount: 5.0,
                    operation: AttributeModifierOperation::AddValue,
                });
            }
            if !self.base().has_custom_name() {
                self.base_mut()
                    .set_custom_name(Some(Component::translatable(
                        "entity.minecraft.killer_bunny",
                        Vec::new(),
                    )));
            }
        } else if let Some(attack) = self
            .mob
            .living
            .attributes
            .get_instance(Attribute::ATTACK_DAMAGE)
        {
            attack.remove_modifier(&EVIL_ATTACK_POWER_MODIFIER);
        }
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_TYPE_ID, variant.wire_id());
    }

    /// `getJumpCompletion(a)`.
    pub fn get_jump_completion(&self, a: f32) -> f32 {
        if self.jump_duration == 0 {
            0.0
        } else {
            (self.jump_ticks as f32 + a) / self.jump_duration as f32
        }
    }

    /// `setSpeedModifier(speed)`.
    pub fn set_speed_modifier(&mut self, level: &mut ServerLevel, speed: f64) {
        navigation::set_speed_modifier(self, level, speed);
        let (x, y, z) = {
            let c = self.mob.move_control.as_deref().expect("move control");
            (c.get_wanted_x(), c.get_wanted_y(), c.get_wanted_z())
        };
        mob::set_wanted_position(self, x, y, z, speed);
    }

    /// `startJumping()`.
    pub fn start_jumping(&mut self, level: &mut ServerLevel) {
        self.set_jumping(level, true);
        self.jump_duration = 15;
        self.jump_ticks = 0;
    }

    /// `wantsMoreFood()`.
    pub fn wants_more_food(&self) -> bool {
        self.more_carrot_ticks <= 0
    }

    fn rabbit_jump_control(&self) -> &RabbitJumpControl {
        self.mob
            .jump_control
            .as_deref()
            .and_then(|c| (c as &dyn std::any::Any).downcast_ref::<RabbitJumpControl>())
            .expect("rabbit jump control")
    }

    fn rabbit_jump_control_mut(&mut self) -> &mut RabbitJumpControl {
        self.mob
            .jump_control
            .as_deref_mut()
            .and_then(|c| (c as &mut dyn std::any::Any).downcast_mut::<RabbitJumpControl>())
            .expect("rabbit jump control")
    }

    /// `facePoint(faceX, faceZ)`.
    fn face_point(&mut self, face_x: f64, face_z: f64) {
        let b = self.base();
        let y_rot = (mth::atan2(face_z - b.z(), face_x - b.x()) * f64::from(180.0f32)
            / f64::from(std::f32::consts::PI)) as f32
            - 90.0;
        self.set_y_rot(y_rot);
    }

    /// `setLandingDelay()`.
    fn set_landing_delay(&mut self) {
        self.jump_delay_ticks = if mob::move_control_speed_modifier(self) < 2.2 {
            10
        } else {
            3
        };
    }

    /// `checkLandingDelay()`.
    fn check_landing_delay(&mut self) {
        self.set_landing_delay();
        self.rabbit_jump_control_mut().set_can_jump(false);
    }

    /// `getRandomRabbitVariant(level, pos)`.
    fn get_random_rabbit_variant(
        level: &mut dyn ServerLevelAccessor,
        pos: BlockPos,
    ) -> RabbitVariant {
        let registries = level.registry_access().clone();
        let biome = level.get_biome_id(pos);
        let random_val = level.random().next_int_bounded(100);
        let in_tag = |tag: &str| {
            let tag = Identifier::parse(tag).expect("valid tag id");
            biome.is_some_and(|b| {
                registries
                    .registry(keys::BIOME)
                    .is_some_and(|r| r.is_in_tag(b.0, &tag))
            })
        };
        if in_tag("minecraft:spawns_white_rabbits") {
            if random_val < 80 {
                RabbitVariant::White
            } else {
                RabbitVariant::WhiteSplotched
            }
        } else if in_tag("minecraft:spawns_gold_rabbits") {
            RabbitVariant::Gold
        } else if random_val < 50 {
            RabbitVariant::Brown
        } else if random_val < 90 {
            RabbitVariant::Salt
        } else {
            RabbitVariant::Black
        }
    }
}

/// `Rabbit.checkRabbitSpawnRules`.
pub fn check_rabbit_spawn_rules(
    _ty: EntityType,
    level: &dyn ServerLevelAccessor,
    _reason: EntitySpawnReason,
    pos: BlockPos,
    _random: &mut dyn RandomSource,
) -> bool {
    level
        .get_block_state(pos.below())
        .is_tag("rabbits_spawnable_on")
        && spawn_predicates::is_bright_enough_to_spawn(level, pos)
}

mc_game_core::register_spawn_placement!(EntityType::RABBIT, check_rabbit_spawn_rules);

impl Entity for Rabbit {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:rabbit")
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() {
            baby_dimensions()
        } else {
            EntityDimensions::of_type(self.mob.living.entity.entity_type)
        }
    }

    fn get_jump_power(&self, level: &ServerLevel) -> f32 {
        let mut base_jump_power = 0.3f32;
        if mob::move_control_speed_modifier(self) <= 0.6 {
            base_jump_power = 0.2;
        }
        if let Some(Some(path)) = navigation::with_navigation_ref(self, level, |n| n.get_path())
            && !path.is_done()
        {
            let current_pos = path.get_next_entity_pos(self.base().get_bb_width());
            if current_pos.y > self.base().y() + 0.5 {
                base_jump_power = 0.5;
            }
        }
        let wanted_y = self
            .mob
            .move_control
            .as_deref()
            .map_or(0.0, |c| c.get_wanted_y());
        if self.base().horizontal_collision
            || (self.mob.living.jumping && wanted_y > self.base().y() + 0.5)
        {
            base_jump_power = 0.5;
        }
        jump::get_jump_power_with(self, level, base_jump_power / 0.42)
    }

    fn jump_from_ground(&mut self, level: &ServerLevel) {
        jump::jump_from_ground(self, level);
        let speed_modifier = mob::move_control_speed_modifier(self);
        if speed_modifier > 0.0 {
            let current = self.base().delta_movement().horizontal_distance_sqr();
            if current < 0.01 {
                let up = if self.is_baby() { 0.5 } else { 1.5 };
                self.move_relative(0.1, Vec3::new(0.0, up, 1.0));
            }
        }
        level.broadcast_entity_event(self.id(), 1);
    }

    fn set_jumping(&mut self, level: &mut ServerLevel, jump: bool) {
        self.mob.living.jumping = jump;
        if jump {
            let volume = self.get_sound_volume();
            let pitch =
                ((self.base_mut().random.next_float() - self.base_mut().random.next_float()) * 0.2
                    + 1.0)
                    * 0.8;
            self.play_sound(level, SoundEvent::ENTITY_RABBIT_JUMP, volume, pitch);
        }
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        if self.jump_delay_ticks > 0 {
            self.jump_delay_ticks -= 1;
        }
        if self.more_carrot_ticks > 0 {
            self.more_carrot_ticks -= self.base_mut().random.next_int_bounded(3);
            if self.more_carrot_ticks < 0 {
                self.more_carrot_ticks = 0;
            }
        }
        if self.base().on_ground() {
            if !self.was_on_ground {
                self.set_jumping(level, false);
                self.check_landing_delay();
            }
            if self.get_variant() == RabbitVariant::Evil
                && self.jump_delay_ticks == 0
                && let Some(target) = self.get_target().and_then(|id| level.get_entity(id))
            {
                let (tx, ty, tz) = (target.base().x(), target.base().y(), target.base().z());
                if self.base().distance_to_sqr_vec(target.position()) < 16.0 {
                    let speed = mob::move_control_speed_modifier(self);
                    self.face_point(tx, tz);
                    mob::set_wanted_position(self, tx, ty, tz, speed);
                    self.start_jumping(level);
                    self.was_on_ground = true;
                }
            }
            if !self.rabbit_jump_control().want_jump() {
                if self
                    .mob
                    .move_control
                    .as_deref()
                    .is_some_and(|c| c.has_wanted())
                    && self.jump_delay_ticks == 0
                {
                    let (wx, wy, wz) = {
                        let c = self.mob.move_control.as_deref().expect("move control");
                        (c.get_wanted_x(), c.get_wanted_y(), c.get_wanted_z())
                    };
                    let mut pos = Vec3::new(wx, wy, wz);
                    if let Some(Some(path)) =
                        navigation::with_navigation_ref(self, level, |n| n.get_path())
                        && !path.is_done()
                    {
                        pos = path.get_next_entity_pos(self.base().get_bb_width());
                    }
                    self.face_point(pos.x, pos.z);
                    self.start_jumping(level);
                }
            } else if !self.rabbit_jump_control().can_jump() {
                self.rabbit_jump_control_mut().set_can_jump(true);
            }
        }
        self.was_on_ground = self.base().on_ground();
    }

    fn can_spawn_sprint_particle(&mut self) -> bool {
        false
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        mob::tick::ai_step(self, level);
        if self.jump_ticks != self.jump_duration {
            self.jump_ticks += 1;
        } else if self.jump_duration != 0 {
            self.jump_ticks = 0;
            self.jump_duration = 0;
            self.set_jumping(level, false);
        }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_int("RabbitType", self.get_variant().wire_id());
        out.put_int("MoreCarrotTicks", self.more_carrot_ticks);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        let variant = input
            .get_int("RabbitType")
            .map_or(DEFAULT_VARIANT, variant_by_id);
        self.set_variant(variant);
        self.more_carrot_ticks = input.get_int_or("MoreCarrotTicks", 0);
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_RABBIT_AMBIENT)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_RABBIT_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_RABBIT_DEATH)
    }

    fn play_attack_sound(&mut self, level: &mut ServerLevel) {
        if self.get_variant() == RabbitVariant::Evil {
            let pitch = (self.base_mut().random.next_float() - self.base_mut().random.next_float())
                * 0.2
                + 1.0;
            self.play_sound(level, SoundEvent::ENTITY_RABBIT_ATTACK, 1.0, pitch);
        }
    }

    fn get_sound_source(&self) -> SoundSource {
        if self.get_variant() == RabbitVariant::Evil {
            SoundSource::Hostile
        } else {
            SoundSource::Neutral
        }
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        let mut offspring = mc_game_core::world::entity::factory::create(
            EntityType::RABBIT,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )?;
        let mut variant = Rabbit::get_random_rabbit_variant(level, self.block_position());
        if self.base_mut().random.next_int_bounded(20) != 0 {
            let partner_variant = level
                .get_entity(partner)
                .and_then(|p| p.as_any().downcast_ref::<Rabbit>())
                .map(Rabbit::get_variant);
            variant = match partner_variant {
                Some(partner_variant) if self.base_mut().random.next_boolean() => partner_variant,
                _ => self.get_variant(),
            };
        }
        if let Some(rabbit) = offspring.as_any_mut().downcast_mut::<Rabbit>() {
            rabbit.set_variant(variant);
        }
        Some(offspring)
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        is_rabbit_food(stack)
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let mut variant = Rabbit::get_random_rabbit_variant(level, self.block_position());
        let group_data = match group_data {
            Some(data) if data.as_any().is::<RabbitGroupData>() => {
                variant = data
                    .as_any()
                    .downcast_ref::<RabbitGroupData>()
                    .expect("rabbit group data")
                    .variant;
                data
            }
            _ => Box::new(RabbitGroupData::new(variant)),
        };
        self.set_variant(variant);
        mob::spawn::finalize_spawn(self, level, difficulty, reason, Some(group_data))
    }

    fn entity_components(&self) -> DataComponentMap {
        let base = mc_game_core::advancements::predicates::entity::support::base_entity_components(
            self.base(),
        );
        DataComponentMap::builder()
            .add_all(&base)
            .set(RABBIT_VARIANT, self.get_variant())
            .build()
    }

    fn apply_implicit_components(&mut self, stack: &ItemStack) {
        if let Some(variant) = stack.get(&RABBIT_VARIANT) {
            self.set_variant(*variant);
        }
        mc_game_core::world::entity::spawn_config::apply_base_implicit_components(self, stack);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static RABBIT: EntityRegistration = EntityRegistration {
    entity_type: EntityType::RABBIT,
    create: |ty, id, registries| Box::new(Rabbit::new(ty, id, registries)),
};
