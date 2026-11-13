//! `Zoglin`.

use super::hoglin::hoglin_base::{
    self, ATTACK_ANIMATION_DURATION, PROBABILITY_OF_SPAWNING_AS_BABY,
};
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{Attribute, EntityType, SoundEvent};
use mc_game_ai::world::entity::ai::behavior::{
    melee_attack, random_stroll, set_entity_look_target_sometimes,
    set_walk_target_from_attack_target_if_target_out_of_reach, start_attacking,
    stop_attacking_if_target_invalid,
};
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::behavior::behavior_utils;
use mc_game_core::world::entity::ai::behavior::look_at_target_sink::LookAtTargetSink;
use mc_game_core::world::entity::ai::behavior::move_to_target_sink::MoveToTargetSink;
use mc_game_core::world::entity::ai::behavior::{
    BehaviorBuilder, BehaviorControl, DoNothing, RunOne, set_walk_target_from_look_target,
};
use mc_game_core::world::entity::ai::brain::{ActivityData, Brain, Provider};
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ATTACK_TARGET, CANT_REACH_WALK_TARGET_SINCE, NEAREST_VISIBLE_LIVING_ENTITIES,
};
use mc_game_core::world::entity::ai::memory::{
    BrainPacked, EntityRef, NearestVisibleLivingEntities,
};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::ai::sensing::sensor::is_entity_attackable;
use mc_game_core::world::entity::ai::sensing::sensor_type::{
    NEAREST_LIVING_ENTITIES, NEAREST_PLAYERS,
};
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::living_entity::damage::make_sound;
use mc_game_core::world::entity::living_entity::util::broadcast_entity_event;
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::schedule::Activity;
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;

define_accessor!(DATA_BABY_ID: Boolean = Zoglin);

const ATTACK_INTERVAL: i32 = 40;
const BABY_ATTACK_INTERVAL: i32 = 15;
const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 0.4;
const BABY_ATTACK_DAMAGE: f64 = 0.5;

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    EntityDimensions::scalable(0.75, 0.85)
        .with_eye_height(0.625)
        .with_attachments(
            EntityAttachments::create_default(0.75, 0.85)
                .with_passenger(Vec3::new(0.0, 0.875, 0.0)),
        )
}

/// `BRAIN_PROVIDER`.
static BRAIN_PROVIDER: Provider = Provider::with_activities(
    &[&NEAREST_LIVING_ENTITIES, &NEAREST_PLAYERS],
    get_activities,
);

fn b(behavior: impl BehaviorControl) -> Box<dyn BehaviorControl> {
    Box::new(behavior)
}

/// `getActivities()`.
fn get_activities(_zoglin: &dyn Entity) -> Vec<ActivityData> {
    vec![
        init_core_activity(),
        init_idle_activity(),
        init_fight_activity(),
    ]
}

fn init_core_activity() -> ActivityData {
    ActivityData::create(
        Activity::CORE,
        0,
        vec![b(LookAtTargetSink::new(45, 90)), b(MoveToTargetSink::new())],
    )
}

fn init_idle_activity() -> ActivityData {
    ActivityData::create(
        Activity::IDLE,
        10,
        vec![
            b(start_attacking::create(find_nearest_valid_attack_target)),
            b(set_entity_look_target_sometimes::create(8.0, 30, 60)),
            b(RunOne::new(vec![
                (b(random_stroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)), 2),
                (
                    b(set_walk_target_from_look_target::create(
                        SPEED_MULTIPLIER_WHEN_IDLING,
                        3,
                    )),
                    2,
                ),
                (b(DoNothing::new(30, 60)), 1),
            ])),
        ],
    )
}

fn init_fight_activity() -> ActivityData {
    ActivityData::create_with_erased_memory(
        Activity::FIGHT,
        10,
        vec![
            b(set_walk_target_from_attack_target_if_target_out_of_reach::create(1.0)),
            b(BehaviorBuilder::trigger_if(
                |e| !e.is_baby(),
                melee_attack::create(ATTACK_INTERVAL),
            )),
            b(BehaviorBuilder::trigger_if(
                |e| e.is_baby(),
                melee_attack::create(BABY_ATTACK_INTERVAL),
            )),
            b(stop_attacking_if_target_invalid::create()),
        ],
        ATTACK_TARGET.id(),
    )
}

/// `findNearestValidAttackTarget(level, mob)`.
fn find_nearest_valid_attack_target(level: &ServerLevel, mob: &dyn Entity) -> Option<EntityId> {
    let visible = mob
        .brain()
        .get_memory(&NEAREST_VISIBLE_LIVING_ENTITIES)
        .cloned()
        .unwrap_or_else(NearestVisibleLivingEntities::empty);
    visible
        .find_closest(level, mob, |target| {
            target.get_type() != EntityType::ZOGLIN
                && target.get_type() != EntityType::CREEPER
                && is_entity_attackable(level, mob, target)
        })
        .map(|r| r.0)
}

pub struct Zoglin {
    pub mob: MobData,
    attack_animation_remaining_ticks: i32,
}

impl Zoglin {
    /// `new Zoglin(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Monster, |b| {
            b.define(DATA_BABY_ID, false);
        });
        let mut zoglin = Zoglin {
            mob,
            attack_animation_remaining_ticks: 0,
        };
        let brain = zoglin.make_brain(&BrainPacked::EMPTY);
        zoglin.mob.living.brain = brain;
        let navigation = StandardPathNavigation::ground(&zoglin);
        zoglin.mob.set_navigation(Box::new(navigation));
        zoglin.mob.xp_reward = 5;
        zoglin
    }

    /// `isAdult()`.
    pub fn is_adult(&self) -> bool {
        !self.is_baby()
    }

    /// `getAttackAnimationRemainingTicks()`.
    pub fn get_attack_animation_remaining_ticks(&self) -> i32 {
        self.attack_animation_remaining_ticks
    }

    /// `setAttackTarget(target)`.
    fn set_attack_target(&mut self, target: EntityId) {
        let brain = self.brain_mut();
        brain.erase_memory(&CANT_REACH_WALK_TARGET_SINCE);
        brain.set_memory_with_expiry(&ATTACK_TARGET, EntityRef(target), 200);
    }

    /// `updateActivity()`.
    fn update_activity(&mut self, level: &mut ServerLevel) {
        let old_activity = self.brain().get_active_non_core_activity();
        self.brain_mut()
            .set_active_activity_to_first_valid(&[Activity::FIGHT, Activity::IDLE]);
        let new_activity = self.brain().get_active_non_core_activity();
        if new_activity == Some(Activity::FIGHT) && old_activity != Some(Activity::FIGHT) {
            self.play_angry_sound(level);
        }
        let attacking = self.brain().has_memory_value(ATTACK_TARGET);
        self.mob.set_aggressive(attacking);
    }

    /// `playAngrySound()`.
    fn play_angry_sound(&mut self, level: &mut ServerLevel) {
        make_sound(self, level, SoundEvent::ENTITY_ZOGLIN_ANGRY);
    }
}

impl Entity for Zoglin {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:zoglin")
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() {
            baby_dimensions()
        } else {
            EntityDimensions::of_type(self.mob.living.entity.entity_type)
        }
    }

    fn on_synced_data_updated(&mut self, accessor_id: u8) {
        if accessor_id == DATA_BABY_ID.id() {
            self.refresh_dimensions();
        }
    }

    fn make_brain(&mut self, packed: &BrainPacked) -> Brain {
        BRAIN_PROVIDER.make_brain(self, packed)
    }

    fn is_baby(&self) -> bool {
        self.base().entity_data.get(DATA_BABY_ID)
    }

    fn set_baby(&mut self, baby: bool) {
        self.base_mut().entity_data.set(DATA_BABY_ID, baby);
        if baby {
            self.mob
                .living
                .attributes
                .get_instance(Attribute::ATTACK_DAMAGE)
                .expect("attack damage")
                .set_base_value(BABY_ATTACK_DAMAGE);
        }
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        if level.random().next_float() < PROBABILITY_OF_SPAWNING_AS_BABY {
            self.set_baby(true);
        }
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn do_hurt_target(&mut self, level: &mut ServerLevel, target: &mut dyn Entity) -> bool {
        if target.living().is_some() {
            self.attack_animation_remaining_ticks = ATTACK_ANIMATION_DURATION;
            broadcast_entity_event(level, self.id(), 4);
            make_sound(self, level, SoundEvent::ENTITY_ZOGLIN_ATTACK);
            hoglin_base::hurt_and_throw_target(level, self, target)
        } else {
            false
        }
    }

    fn blocked_by_item(
        &mut self,
        level: &mut ServerLevel,
        defender: &mut dyn Entity,
        _source: &DamageSource,
        _damage: f32,
        _fully_blocked: bool,
    ) {
        if self.is_adult() {
            hoglin_base::throw_target(level, &*self, defender);
        }
    }

    fn can_be_leashed(&self) -> bool {
        true
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        let was_hurt = mc_game_core::world::entity::living_entity::damage::hurt_server(
            self, level, source, damage,
        );
        if was_hurt && source.causing_is_living() {
            mob::ext::with_causing_entity(level, source, |level, attacker| {
                if self.can_attack(level, attacker)
                    && !behavior_utils::is_other_target_much_further_away_than_current_attack_target(
                        level, &*self, attacker, 4.0,
                    )
                {
                    self.set_attack_target(attacker.id());
                }
            });
            return true;
        }
        was_hurt
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        Brain::tick(level, self);
        self.update_activity(level);
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        if self.attack_animation_remaining_ticks > 0 {
            self.attack_animation_remaining_ticks -= 1;
        }
        mob::tick::ai_step(self, level);
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(if self.brain().has_memory_value(ATTACK_TARGET) {
            SoundEvent::ENTITY_ZOGLIN_ANGRY
        } else {
            SoundEvent::ENTITY_ZOGLIN_AMBIENT
        })
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ZOGLIN_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ZOGLIN_DEATH)
    }

    fn play_step_sound(
        &mut self,
        level: &mut ServerLevel,
        _pos: BlockPos,
        _state: mc_data::block::BlockState,
    ) {
        self.play_sound(level, SoundEvent::ENTITY_ZOGLIN_STEP, 0.15, 1.0);
    }

    fn get_target(&self) -> Option<EntityId> {
        self.brain().get_memory(&ATTACK_TARGET).map(|r| r.0)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_boolean("IsBaby", self.is_baby());
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.set_baby(input.get_boolean_or("IsBaby", false));
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static ZOGLIN: EntityRegistration = EntityRegistration {
    entity_type: EntityType::ZOGLIN,
    create: |ty, id, registries| Box::new(Zoglin::new(ty, id, registries)),
};
