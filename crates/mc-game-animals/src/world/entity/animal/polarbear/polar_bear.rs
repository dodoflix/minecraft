//! `PolarBear`: neutral bear that defends its cubs and hunts foxes.

use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::{DamageSource, DamageTypeTags};
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::goal::target::{HurtByTargetGoal, NearestAttackableTargetGoal, TargetType};
use mc_game_ai::world::entity::ai::goal::ResetUniversalAngerTargetGoal;
use mc_game_core::world::entity::ai::goal::{FloatGoal, FollowParentGoal, Goal, GoalFlags, LookAtPlayerGoal, MeleeAttackGoal, PanicGoal, RandomLookAroundGoal, RandomStrollGoal};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest};
use mc_game_core::world::entity::mob::neutral::{self, NeutralData};
use mc_game_core::world::entity::mob::spawn::AgeableMobGroupData;
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::spawn_predicates;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::block::BlockStateExt as _;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_game_core::world::level::BlockGetter as _;
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{EntityType, SoundEvent};
use mc_nbt::CompoundTag;
use std::sync::Arc;

define_accessor!(pub DATA_STANDING_ID: Boolean = PolarBear);

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    let dims = EntityDimensions::scalable(0.7, 0.7).with_eye_height(0.34375);
    dims.with_attachments(EntityAttachments::create_default(0.7, 0.7).with_passenger(Vec3::new(0.0, 0.625, 0.0)))
}

fn bear_mut(e: &mut dyn Entity) -> &mut PolarBear {
    e.as_any_mut().downcast_mut::<PolarBear>().expect("PolarBear")
}

/// `PolarBear.PolarBearMeleeAttackGoal`.
struct PolarBearMeleeAttackGoal {
    melee: MeleeAttackGoal,
}

impl Goal for PolarBearMeleeAttackGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.melee.can_use(mob, level)
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.melee.can_continue_to_use(mob, level)
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.melee.start(mob, level);
    }
    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        bear_mut(mob).set_standing(false);
        self.melee.stop(mob, level);
    }
    fn requires_update_every_tick(&self) -> bool {
        true
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.melee.tick_with(mob, level, &mut |goal, mob, level, target_id| {
            if goal.can_perform_attack(mob, level, target_id) {
                goal.reset_attack_cooldown();
                mob::combat::do_hurt_target(mob, level, target_id);
                bear_mut(mob).set_standing(false);
                return;
            }
            let Some(target) = level.get_entity(target_id) else { return };
            let width = target.base().get_bb_width();
            let within = mob.base().distance_to_sqr_vec(target.position()) < f64::from((width + 3.0) * (width + 3.0));
            if within {
                if goal.is_time_to_attack() {
                    bear_mut(mob).set_standing(false);
                    goal.reset_attack_cooldown();
                }
                if goal.get_ticks_until_next_attack() <= 10 {
                    bear_mut(mob).set_standing(true);
                    bear_mut(mob).play_warning_sound(level);
                }
            } else {
                goal.reset_attack_cooldown();
                bear_mut(mob).set_standing(false);
            }
        });
    }
    fn flags(&self) -> GoalFlags {
        self.melee.flags()
    }
}

/// `PolarBear.PolarBearHurtByTargetGoal`.
struct PolarBearHurtByTargetGoal {
    hurt: HurtByTargetGoal,
}

impl PolarBearHurtByTargetGoal {
    fn new() -> Self {
        let mut hurt = HurtByTargetGoal::new(vec![]);
        hurt.alert_other = Some(Box::new(|level, other, hurt_by| {
            level.with_entity(other, |level, o| {
                if o.as_any().is::<PolarBear>() && !o.is_baby() {
                    o.set_target(level, Some(hurt_by));
                }
            });
        }));
        Self { hurt }
    }
}

impl Goal for PolarBearHurtByTargetGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.hurt.can_use(mob, level)
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.hurt.can_continue_to_use(mob, level)
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.hurt.start(mob, level);
        if mob.is_baby() {
            self.hurt.alert_others(mob, level);
            self.hurt.stop(mob, level);
        }
    }
    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.hurt.stop(mob, level);
    }
    fn flags(&self) -> GoalFlags {
        self.hurt.flags()
    }
}

/// `PolarBear.PolarBearAttackPlayersGoal`.
struct PolarBearAttackPlayersGoal {
    nearest: NearestAttackableTargetGoal,
}

impl PolarBearAttackPlayersGoal {
    fn new() -> Self {
        let mut nearest = NearestAttackableTargetGoal::build(TargetType::Player, 20, true, true, None);
        nearest.target_goal.follow_distance_factor = 0.5;
        Self { nearest }
    }
}

impl Goal for PolarBearAttackPlayersGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if mob.is_baby() {
            return false;
        }
        if self.nearest.can_use_impl(mob, level) {
            let bb = mob.base().bounding_box().inflate(8.0, 4.0, 8.0);
            return level.get_entities_of_class(&bb, |e| e.as_any().is::<PolarBear>(), |e| e.is_baby()).into_iter().next().is_some();
        }
        false
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.nearest.can_continue_to_use(mob, level)
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.nearest.start(mob, level);
    }
    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.nearest.stop(mob, level);
    }
    fn flags(&self) -> GoalFlags {
        self.nearest.flags()
    }
}

/// `PolarBear`.
pub struct PolarBear {
    pub mob: MobData,
    warning_sound_ticks: i32,
}

impl PolarBear {
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_STANDING_ID, false);
        });
        let mut bear = PolarBear { mob, warning_sound_ticks: 0 };
        bear.mob.neutral = Some(NeutralData::default());
        let navigation = StandardPathNavigation::ground(&bear);
        bear.mob.set_navigation(Box::new(navigation));
        bear.register_goals();
        bear
    }

    fn register_goals(&mut self) {
        let float = FloatGoal::new(self);
        let follow_parent = FollowParentGoal::new(self, 1.25);
        let look_at = LookAtPlayerGoal::new_player(self, 6.0);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(0, Box::new(float));
        goals.add_goal(1, Box::new(PolarBearMeleeAttackGoal { melee: MeleeAttackGoal::new(1.25, true) }));
        goals.add_goal(1, Box::new(PanicGoal::with_tag_fn(2.0, |bear| if bear.is_baby() { DamageTypeTags::PANIC_CAUSES } else { DamageTypeTags::PANIC_ENVIRONMENTAL_CAUSES })));
        goals.add_goal(4, Box::new(follow_parent));
        goals.add_goal(5, Box::new(RandomStrollGoal::new(1.0)));
        goals.add_goal(6, Box::new(look_at));
        goals.add_goal(7, Box::new(RandomLookAroundGoal::new()));
        let targets = &mut self.mob.target_selector;
        targets.add_goal(1, Box::new(PolarBearHurtByTargetGoal::new()));
        targets.add_goal(2, Box::new(PolarBearAttackPlayersGoal::new()));
        targets.add_goal(3, Box::new(NearestAttackableTargetGoal::build(TargetType::Player, 10, true, false, None).with_source_selector(Arc::new(|mob, target, level| neutral::is_angry_at(mob, level, target)))));
        targets.add_goal(
            4,
            Box::new(NearestAttackableTargetGoal::build(TargetType::of_type(EntityType::FOX), 10, true, true, None).with_source_selector(Arc::new(|mob, _target, _level| !mob.is_baby()))),
        );
        targets.add_goal(5, Box::new(ResetUniversalAngerTargetGoal::new(false)));
    }

    pub fn is_standing(&self) -> bool {
        self.mob.living.entity.entity_data.get(DATA_STANDING_ID)
    }

    pub fn set_standing(&mut self, value: bool) {
        self.mob.living.entity.entity_data.set(DATA_STANDING_ID, value);
    }

    /// `playWarningSound()`.
    fn play_warning_sound(&mut self, level: &mut ServerLevel) {
        if self.warning_sound_ticks <= 0 {
            mc_game_core::world::entity::living_entity::damage::make_sound(self, level, SoundEvent::ENTITY_POLAR_BEAR_WARNING);
            self.warning_sound_ticks = 40;
        }
    }
}

impl Entity for PolarBear {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:polar_bear")
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() { baby_dimensions() } else { EntityDimensions::of_type(self.mob.living.entity.entity_type) }
    }

    fn get_breed_offspring(&mut self, level: &mut ServerLevel, _partner: EntityId) -> Option<Box<dyn Entity>> {
        mc_game_core::world::entity::factory::create(EntityType::POLAR_BEAR, level, EntitySpawnRequest { reason: EntitySpawnReason::Breeding, ignore_checks: false })
    }

    fn is_food(&self, _stack: &ItemStack) -> bool {
        false
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        neutral::read_persistent_anger_save_data(self, None, input);
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        neutral::add_persistent_anger_save_data(self, out);
    }

    fn start_persistent_anger_timer(&mut self, level: &mut ServerLevel) {
        let time = self.base_mut().random.next_int_between_inclusive(400, 780);
        neutral::set_time_to_remain_angry(self, level, i64::from(time));
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_baby() { SoundEvent::ENTITY_POLAR_BEAR_AMBIENT_BABY } else { SoundEvent::ENTITY_POLAR_BEAR_AMBIENT })
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_POLAR_BEAR_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_POLAR_BEAR_DEATH)
    }

    fn play_step_sound(&mut self, level: &mut ServerLevel, _pos: BlockPos, _state: mc_data::block::BlockState) {
        self.play_sound(level, SoundEvent::ENTITY_POLAR_BEAR_STEP, 0.15, 1.0);
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        mob::tick::tick(self, level);
        if self.warning_sound_ticks > 0 {
            self.warning_sound_ticks -= 1;
        }
        neutral::update_persistent_anger(self, level, true);
    }

    fn get_water_slow_down(&self) -> f32 {
        0.98
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let group_data = group_data.or_else(|| Some(Box::new(AgeableMobGroupData::with_chance(1.0))));
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }
}

/// `PolarBear.checkPolarBearSpawnRules`.
fn check_polar_bear_spawn_rules(ty: EntityType, level: &dyn ServerLevelAccessor, reason: EntitySpawnReason, pos: BlockPos, random: &mut dyn RandomSource) -> bool {
    if !level.biome_is_tag(pos, "polar_bears_spawn_on_alternate_blocks") {
        spawn_predicates::check_animal_spawn_rules(ty, level, reason, pos, random)
    } else {
        spawn_predicates::is_bright_enough_to_spawn(level, pos) && level.get_block_state(pos.below()).is_tag("polar_bears_spawnable_on_alternate")
    }
}

mc_game_core::register_spawn_placement!(EntityType::POLAR_BEAR, check_polar_bear_spawn_rules);

#[distributed_slice(ENTITY_FACTORIES)]
static POLAR_BEAR: EntityRegistration = EntityRegistration { entity_type: EntityType::POLAR_BEAR, create: |ty, id, registries| Box::new(PolarBear::new(ty, id, registries)) };
