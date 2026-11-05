//! `net.minecraft.world.entity.monster.spider.Spider` and `CaveSpider` (a `Spider` subclass that only overrides
//! `doHurtTarget`, `finalizeSpawn` and the vehicle attachment point, so one struct serves both entity types).

use crate::define_accessor;
use crate::impl_spawn_group_data;
use crate::registry::RegistryAccess;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::ai::goal::target::{EntityClass, HurtByTargetGoal, NearestAttackableTargetGoal, TargetType, class_of_type};
use crate::world::entity::ai::goal::{AvoidEntityGoal, FloatGoal, Goal, GoalFlags, LeapAtTargetGoal, LookAtPlayerGoal, MeleeAttackGoal, RandomLookAroundGoal, WaterAvoidingRandomStrollGoal};
use crate::world::entity::ai::navigation::StandardPathNavigation;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest};
use crate::world::entity::mob::{self, MobData, MobKind};
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::enums::EntitySpawnReason;
use mc_data::{Attribute, EntityType, MobEffect, SoundEvent};
use std::sync::Arc;

define_accessor!(pub DATA_FLAGS_ID: Byte = Spider);

const SPIDER_SPECIAL_EFFECT_CHANCE: f32 = 0.1;

/// `Spider.SpiderEffectsGroupData`.
#[derive(Default)]
pub struct SpiderEffectsGroupData {
    pub effect: Option<MobEffect>,
}

impl SpiderEffectsGroupData {
    /// `setRandomEffect(random)`.
    pub fn set_random_effect(&mut self, random: &mut dyn RandomSource) {
        let selection = random.next_int_bounded(5);
        if selection <= 1 {
            self.effect = Some(MobEffect::SPEED);
        } else if selection <= 2 {
            self.effect = Some(MobEffect::STRENGTH);
        } else if selection <= 3 {
            self.effect = Some(MobEffect::REGENERATION);
        } else if selection <= 4 {
            self.effect = Some(MobEffect::INVISIBILITY);
        }
    }
}

impl_spawn_group_data!(SpiderEffectsGroupData);

/// `Spider` / `CaveSpider`.
pub struct Spider {
    pub mob: MobData,
}

impl Spider {
    /// `new Spider(type, level)` / `new CaveSpider(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Monster, |b| {
            b.define(DATA_FLAGS_ID, 0);
        });
        let mut spider = Spider { mob };
        let navigation = StandardPathNavigation::wall_climber(&spider);
        spider.mob.set_navigation(Box::new(navigation));
        spider.register_goals();
        spider
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let float = FloatGoal::new(self);
        let look_at = LookAtPlayerGoal::new_player(self, 8.0);
        let armadillo: EntityClass = class_of_type(EntityType::ARMADILLO);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(1, Box::new(float));
        goals.add_goal(2, Box::new(AvoidEntityGoal::with_predicate_on_avoid_entity(armadillo, 6.0, 1.0, 1.2, Arc::new(|e: &dyn Entity| !e.armadillo_is_scared()))));
        goals.add_goal(3, Box::new(LeapAtTargetGoal::new(0.4)));
        goals.add_goal(4, Box::new(SpiderAttackGoal::new()));
        goals.add_goal(5, Box::new(WaterAvoidingRandomStrollGoal::new(0.8)));
        goals.add_goal(6, Box::new(look_at));
        goals.add_goal(6, Box::new(RandomLookAroundGoal::new()));
        let targets = &mut self.mob.target_selector;
        targets.add_goal(1, Box::new(HurtByTargetGoal::new(Vec::new())));
        targets.add_goal(2, Box::new(SpiderTargetGoal::new(TargetType::Player)));
        targets.add_goal(3, Box::new(SpiderTargetGoal::new(TargetType::of_type(EntityType::IRON_GOLEM))));
    }

    fn is_cave_spider(&self) -> bool {
        self.mob.living.entity.entity_type == EntityType::CAVE_SPIDER
    }

    /// `isClimbing()`.
    pub fn is_climbing(&self) -> bool {
        (self.mob.living.entity.entity_data.get(DATA_FLAGS_ID) & 1) != 0
    }

    /// `setClimbing(value)`.
    pub fn set_climbing(&mut self, value: bool) {
        let mut flags = self.mob.living.entity.entity_data.get(DATA_FLAGS_ID);
        if value {
            flags |= 1;
        } else {
            flags &= -2;
        }
        self.mob.living.entity.entity_data.set(DATA_FLAGS_ID, flags);
    }
}

impl Entity for Spider {
    crate::mob_entity_accessors!(mob);

    fn tick(&mut self, level: &mut ServerLevel) {
        mob::tick::tick(self, level);
        self.set_climbing(self.mob.living.entity.horizontal_collision);
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SPIDER_AMBIENT)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SPIDER_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SPIDER_DEATH)
    }

    fn play_step_sound(&mut self, level: &mut ServerLevel, _pos: BlockPos, _state: mc_data::block::BlockState) {
        self.play_sound(level, SoundEvent::ENTITY_SPIDER_STEP, 0.15, 1.0);
    }

    fn on_climbable(&self, _level: &ServerLevel) -> bool {
        self.is_climbing()
    }

    fn make_stuck_in_block(&mut self, state: mc_data::block::BlockState, speed_multiplier: Vec3) {
        if state.block() != Block::COBWEB {
            self.reset_fall_distance();
            self.base_mut().stuck_speed_multiplier = speed_multiplier;
        }
    }

    fn can_be_affected(&self, level: &ServerLevel, effect: &MobEffectInstance) -> bool {
        if effect.effect == MobEffect::POISON { false } else { crate::world::entity::living_entity::effects::can_be_affected(self, level, effect) }
    }

    fn do_hurt_target(&mut self, level: &mut ServerLevel, target: &mut dyn Entity) -> bool {
        if !crate::world::entity::living_entity::combat::mob_do_hurt_target(self, level, target) {
            return false;
        }
        if self.is_cave_spider() && target.living().is_some() {
            let poison_time = match level.difficulty() {
                mc_world::level_storage::Difficulty::Normal => 7,
                mc_world::level_storage::Difficulty::Hard => 15,
                _ => 0,
            };
            if poison_time > 0 {
                let id = self.id();
                target.add_effect(level, MobEffectInstance::of(MobEffect::POISON, poison_time * 20, 0), Some(id));
            }
        }
        true
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        if self.is_cave_spider() {
            return group_data;
        }
        let mut group_data = mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data);
        if level.random().next_int_bounded(100) == 0 {
            if let Some(mut skeleton) = level.create_entity(EntityType::SKELETON, EntitySpawnRequest { reason: EntitySpawnReason::Jockey, ignore_checks: false }) {
                let b = self.base();
                skeleton.snap_to(b.x(), b.y(), b.z(), b.y_rot(), 0.0);
                skeleton.finalize_spawn(level, difficulty, reason, None);
                if crate::world::entity::monster::start_riding_unadded(skeleton.as_mut(), self) {
                    self.mob.pending_passengers.push(skeleton);
                }
            }
        }
        if group_data.is_none() {
            let mut data = SpiderEffectsGroupData::default();
            if level.get_difficulty() == mc_world::level_storage::Difficulty::Hard && level.random().next_float() < SPIDER_SPECIAL_EFFECT_CHANCE * difficulty.get_special_multiplier() {
                data.set_random_effect(level.random());
            }
            group_data = Some(Box::new(data));
        }
        let effect = group_data.as_ref().and_then(|g| g.as_any().downcast_ref::<SpiderEffectsGroupData>()).and_then(|d| d.effect);
        if let (Some(effect), Some(server_level)) = (effect, level.as_server_level_mut()) {
            self.add_effect(server_level, MobEffectInstance::of(effect, -1, 0), None);
        }
        group_data
    }

    fn get_vehicle_attachment_point(&self, vehicle: &dyn Entity) -> Vec3 {
        if vehicle.base().get_bb_width() <= self.base().get_bb_width() {
            let scale = self.mob.living.get_attribute_value(Attribute::SCALE) as f32;
            let y = if self.is_cave_spider() { 0.21875 } else { 0.3125 };
            Vec3::new(0.0, y * f64::from(scale), 0.0)
        } else {
            self.get_attachments().get(crate::world::entity::dimensions::EntityAttachment::Vehicle, 0, self.base().y_rot())
        }
    }
}

/// `Spider.SpiderAttackGoal`.
struct SpiderAttackGoal {
    melee: MeleeAttackGoal,
}

impl SpiderAttackGoal {
    fn new() -> Self {
        Self { melee: MeleeAttackGoal::new(1.0, true) }
    }
}

impl Goal for SpiderAttackGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.melee.can_use_impl(mob, level) && !mob.base().is_vehicle()
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let br = mob::ext::get_light_level_dependent_magic_value_at_entity(level, mob);
        if br >= 0.5 && mob.base_mut().random.next_int_bounded(100) == 0 {
            mob.set_target(level, None);
            false
        } else {
            self.melee.can_continue_to_use_impl(mob, level)
        }
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.melee.start_impl(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.melee.stop_impl(mob, level);
    }

    fn requires_update_every_tick(&self) -> bool {
        self.melee.requires_update_every_tick()
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.melee.tick_impl(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.melee.flags()
    }
}

/// `Spider.SpiderTargetGoal`.
struct SpiderTargetGoal {
    target: NearestAttackableTargetGoal,
}

impl SpiderTargetGoal {
    fn new(target_type: TargetType) -> Self {
        Self { target: NearestAttackableTargetGoal::new(target_type, true) }
    }
}

impl Goal for SpiderTargetGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let br = mob::ext::get_light_level_dependent_magic_value_at_entity(level, mob);
        if br >= 0.5 { false } else { self.target.can_use_impl(mob, level) }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.target.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.target.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.target.stop(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.target.flags()
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static SPIDER: EntityRegistration = EntityRegistration { entity_type: EntityType::SPIDER, create: |ty, id, registries| Box::new(Spider::new(ty, id, registries)) };

#[distributed_slice(ENTITY_FACTORIES)]
static CAVE_SPIDER: EntityRegistration = EntityRegistration { entity_type: EntityType::CAVE_SPIDER, create: |ty, id, registries| Box::new(Spider::new(ty, id, registries)) };
