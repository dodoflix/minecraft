//! `Hoglin`.

use super::hoglin_ai::{self, is_pacified};
use super::hoglin_base::{self, ATTACK_ANIMATION_DURATION, PROBABILITY_OF_SPAWNING_AS_BABY};
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{Attribute, EntityType, Item, MobEffect, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::brain::{Brain, Provider};
use mc_game_core::world::entity::ai::memory::BrainPacked;
use mc_game_core::world::entity::ai::memory::memory_module_type::ATTACK_TARGET;
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::ai::sensing::sensor_type::{
    HOGLIN_SPECIFIC_SENSOR, NEAREST_ADULT, NEAREST_LIVING_ENTITIES, NEAREST_PLAYERS,
};
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::effect::MobEffectInstance;
use mc_game_core::world::entity::factory::{
    self, ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::living_entity::damage::make_sound;
use mc_game_core::world::entity::living_entity::util::broadcast_entity_event;
use mc_game_core::world::entity::mob::convert::{self, ConversionParams};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::SoundSource;

define_accessor!(DATA_IMMUNE_TO_ZOMBIFICATION: Boolean = Hoglin);

const CONVERSION_TIME: i32 = 300;
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
    &[
        &NEAREST_LIVING_ENTITIES,
        &NEAREST_PLAYERS,
        &NEAREST_ADULT,
        &HOGLIN_SPECIFIC_SENSOR,
    ],
    hoglin_ai::get_activities,
);

pub struct Hoglin {
    pub mob: MobData,
    attack_animation_remaining_ticks: i32,
    time_in_overworld: i32,
    cannot_be_hunted: bool,
}

impl Hoglin {
    /// `new Hoglin(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_IMMUNE_TO_ZOMBIFICATION, false);
        });
        let mut hoglin = Hoglin {
            mob,
            attack_animation_remaining_ticks: 0,
            time_in_overworld: 0,
            cannot_be_hunted: false,
        };
        hoglin.mob.enemy = true;
        let brain = hoglin.make_brain(&BrainPacked::EMPTY);
        hoglin.mob.living.brain = brain;
        let navigation = StandardPathNavigation::ground(&hoglin);
        hoglin.mob.set_navigation(Box::new(navigation));
        hoglin.mob.xp_reward = 5;
        hoglin
    }

    /// `isAdult()`.
    pub fn is_adult(&self) -> bool {
        !self.is_baby()
    }

    /// The part of `ageBoundaryReached()` that sets the experience reward and attack damage of a baby or an adult.
    fn apply_age_stats(&mut self) {
        let (xp_reward, attack_damage) = if self.is_baby() {
            (3, BABY_ATTACK_DAMAGE)
        } else {
            (5, 6.0)
        };
        self.mob.xp_reward = xp_reward;
        self.mob
            .living
            .attributes
            .get_instance(Attribute::ATTACK_DAMAGE)
            .expect("attack damage")
            .set_base_value(attack_damage);
    }

    fn is_immune_to_zombification(&self) -> bool {
        self.base().entity_data.get(DATA_IMMUNE_TO_ZOMBIFICATION)
    }

    /// `setImmuneToZombification(isImmuneToZombification)`.
    pub fn set_immune_to_zombification(&mut self, immune: bool) {
        self.base_mut()
            .entity_data
            .set(DATA_IMMUNE_TO_ZOMBIFICATION, immune);
    }

    /// `isConverting()`.
    pub fn is_converting(&self, level: &ServerLevel) -> bool {
        !self.is_immune_to_zombification()
            && !self.mob.is_no_ai()
            && level.attribute_bool_vec(
                mc_game_core::world::attribute::attributes::PIGLINS_ZOMBIFY,
                &self.position(),
            )
    }

    fn set_cannot_be_hunted(&mut self, cannot_be_hunted: bool) {
        self.cannot_be_hunted = cannot_be_hunted;
    }

    /// `getAttackAnimationRemainingTicks()`.
    pub fn get_attack_animation_remaining_ticks(&self) -> i32 {
        self.attack_animation_remaining_ticks
    }

    /// `finishConversion()`.
    fn finish_conversion(&mut self, level: &mut ServerLevel) {
        let params = ConversionParams::single(&*self, level, true, false);
        convert::convert_to(
            self,
            level,
            EntityType::ZOGLIN,
            &params,
            EntitySpawnReason::Conversion,
            |zoglin, level| {
                zoglin.add_effect(
                    level,
                    MobEffectInstance::of(MobEffect::NAUSEA, 200, 0),
                    None,
                );
            },
        );
    }
}

impl Entity for Hoglin {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:hoglin")
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() {
            baby_dimensions()
        } else {
            EntityDimensions::of_type(self.mob.living.entity.entity_type)
        }
    }

    fn make_brain(&mut self, packed: &BrainPacked) -> Brain {
        BRAIN_PROVIDER.make_brain(self, packed)
    }

    fn can_be_leashed(&self) -> bool {
        true
    }

    fn do_hurt_target(&mut self, level: &mut ServerLevel, target: &mut dyn Entity) -> bool {
        if target.living().is_some() {
            self.attack_animation_remaining_ticks = ATTACK_ANIMATION_DURATION;
            broadcast_entity_event(level, self.id(), 4);
            make_sound(self, level, SoundEvent::ENTITY_HOGLIN_ATTACK);
            hoglin_ai::on_hit_target(level, self, &*target);
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
        if !self.is_baby() {
            hoglin_base::throw_target(level, &*self, defender);
        }
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        let was_hurt = mc_game_core::world::entity::living_entity::damage::hurt_server(
            self, level, source, damage,
        );
        if was_hurt && source.causing_is_living() {
            mob::ext::with_causing_entity(level, source, |level, attacker| {
                hoglin_ai::was_hurt_by(level, self, attacker)
            });
        }
        was_hurt
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        Brain::tick(level, self);
        hoglin_ai::update_activity(level, self);
        if self.is_converting(level) {
            self.time_in_overworld = self.time_in_overworld.wrapping_add(1);
            if self.time_in_overworld > CONVERSION_TIME {
                make_sound(
                    self,
                    level,
                    SoundEvent::ENTITY_HOGLIN_CONVERTED_TO_ZOMBIFIED,
                );
                self.finish_conversion(level);
            }
        } else {
            self.time_in_overworld = 0;
        }
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        if self.attack_animation_remaining_ticks > 0 {
            self.attack_animation_remaining_ticks -= 1;
        }
        mob::tick::ai_step(self, level);
    }

    fn age_boundary_reached(&mut self, _level: &mut ServerLevel) {
        self.apply_age_stats();
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

    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        true
    }

    fn get_walk_target_value(
        &self,
        pos: BlockPos,
        level: &dyn mc_game_core::world::level::LevelReader,
    ) -> f32 {
        if hoglin_ai::is_pos_near_nearest_repellent(self, pos) {
            -1.0
        } else if level.get_block_state(pos.below()).block() == Block::CRIMSON_NYLIUM {
            10.0
        } else {
            0.0
        }
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut mc_game_core::world::entity::ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let interaction_succeeded = mob::interact::mob_interact(self, level, player, hand);
        if interaction_succeeded.consumes_action() {
            self.set_persistence_required();
        }
        interaction_succeeded
    }

    fn should_drop_experience(&self) -> bool {
        true
    }

    fn get_base_experience_reward(&mut self, _level: &ServerLevel) -> i32 {
        self.mob.xp_reward
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        mc_data::tags::get::<Item>("hoglin_food").is_some_and(|t| t.contains(item_stack.item()))
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_boolean("IsImmuneToZombification", self.is_immune_to_zombification());
        out.put_int("TimeInOverworld", self.time_in_overworld);
        out.put_boolean("CannotBeHunted", self.cannot_be_hunted);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        if self.is_baby() {
            // `setAge` crossed the baby boundary while the data loaded
            self.apply_age_stats();
        }
        self.set_immune_to_zombification(input.get_boolean_or("IsImmuneToZombification", false));
        self.time_in_overworld = input.get_int_or("TimeInOverworld", 0);
        self.set_cannot_be_hunted(input.get_boolean_or("CannotBeHunted", false));
    }

    fn can_be_hunted(&self) -> bool {
        self.is_adult() && !self.cannot_be_hunted
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        _partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        let mut offspring = factory::create(
            EntityType::HOGLIN,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )?;
        offspring.set_persistence_required();
        Some(offspring)
    }

    fn can_fall_in_love(&self) -> bool {
        !is_pacified(self) && mob::animal::can_fall_in_love(self)
    }

    fn get_sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    fn play_ambient_sound(&mut self, level: &mut ServerLevel) {
        // `getAmbientSound()`: the activity sound (needs the level for `isConverting`)
        if let Some(sound) = hoglin_ai::get_sound_for_current_activity(level, self) {
            make_sound(self, level, sound);
        }
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_HOGLIN_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_HOGLIN_DEATH)
    }

    fn get_swim_sound(&self) -> SoundEvent {
        SoundEvent::ENTITY_HOSTILE_SWIM
    }

    fn get_swim_splash_sound(&self) -> SoundEvent {
        SoundEvent::ENTITY_HOSTILE_SPLASH
    }

    fn play_step_sound(
        &mut self,
        level: &mut ServerLevel,
        _pos: BlockPos,
        _state: mc_data::block::BlockState,
    ) {
        self.play_sound(level, SoundEvent::ENTITY_HOGLIN_STEP, 0.15, 1.0);
    }

    fn get_target(&self) -> Option<EntityId> {
        self.brain().get_memory(&ATTACK_TARGET).map(|r| r.0)
    }
}

/// `Hoglin.checkHoglinSpawnRules(type, level, spawnReason, pos, random)`.
fn check_hoglin_spawn_rules(
    _ty: EntityType,
    level: &dyn ServerLevelAccessor,
    _reason: EntitySpawnReason,
    pos: BlockPos,
    _random: &mut dyn RandomSource,
) -> bool {
    level.get_block_state(pos.below()).block() != Block::NETHER_WART_BLOCK
}

mc_game_core::register_spawn_placement!(EntityType::HOGLIN, check_hoglin_spawn_rules);

#[distributed_slice(ENTITY_FACTORIES)]
static HOGLIN: EntityRegistration = EntityRegistration {
    entity_type: EntityType::HOGLIN,
    create: |ty, id, registries| Box::new(Hoglin::new(ty, id, registries)),
};
