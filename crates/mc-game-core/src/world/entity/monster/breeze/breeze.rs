//! `net.minecraft.world.entity.monster.breeze.Breeze`.
//!
//! Client-only parts (animation states, jump trail / ground particles, `playAmbientSound`, `playWhirlSound`'s sound) are
//! not ported; their random draws that also run on the server are kept (`tick`).
//!
//! shortcut: `PROJECTILE_DEFLECTION` plays `BREEZE_DEFLECT` before reversing; `deflection()` has no level to play it with,
//! so the reversal is returned without the sound; upgrade with a level-aware deflection hook.

use super::breeze_ai;
use crate::registry::RegistryAccess;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::ai::brain::{Brain, Provider};
use crate::world::entity::ai::memory::BrainPacked;
use crate::world::entity::ai::navigation::StandardPathNavigation;
use crate::world::entity::ai::sensing::sensor_type::{BREEZE_ATTACK_ENTITY_SENSOR, HURT_BY as HURT_BY_SENSOR, NEAREST_LIVING_ENTITIES, NEAREST_PLAYERS};
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::living_entity::damage;
use crate::world::entity::mob::{self, MobData, MobKind};
use crate::world::entity::projectile::{Projectile, ProjectileDeflection};
use crate::world::entity::schedule::Activity;
use crate::world::entity::{Entity, EntityId, MovementEmission};
use crate::world::level::pathfinder::PathType;
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_data::enums::Pose;
use mc_data::{EntityType, SoundEvent};

const FALL_DISTANCE_SOUND_TRIGGER_THRESHOLD: f64 = 3.0;
const WHIRL_SOUND_FREQUENCY_MIN: i32 = 1;
const WHIRL_SOUND_FREQUENCY_MAX: i32 = 80;

/// `BRAIN_PROVIDER`.
static BRAIN_PROVIDER: Provider = Provider::with_activities(&[&NEAREST_LIVING_ENTITIES, &HURT_BY_SENSOR, &NEAREST_PLAYERS, &BREEZE_ATTACK_ENTITY_SENSOR], breeze_ai::get_activities);

/// `Breeze`.
pub struct Breeze {
    pub mob: MobData,
    sound_tick: i32,
}

impl Breeze {
    /// `new Breeze(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Monster, |_| {});
        let mut breeze = Breeze { mob, sound_tick: 0 };
        // `LivingEntity.<init>`: `this.brain = this.makeBrain(Brain.Packed.EMPTY)`
        let brain = breeze.make_brain(&BrainPacked::EMPTY);
        breeze.mob.living.brain = brain;
        let navigation = StandardPathNavigation::ground(&breeze);
        breeze.mob.set_navigation(Box::new(navigation));
        breeze.set_pathfinding_malus(PathType::OnTopOfTrapdoor, -1.0);
        breeze.set_pathfinding_malus(PathType::Fire, -1.0);
        breeze.mob.xp_reward = 10;
        breeze
    }

    /// `getFiringYPosition()`.
    pub fn get_firing_y_position(&self) -> f64 {
        self.base().y() + f64::from(self.base().dimensions.height / 2.0) + f64::from(0.3f32)
    }

    /// `withinInnerCircleRange(target)`.
    pub fn within_inner_circle_range(&self, target: Vec3) -> bool {
        let our_position = Vec3::at_center_of(&self.base().block_position());
        target.closer_than_xz_y(our_position, 4.0, 10.0)
    }
}

impl Entity for Breeze {
    crate::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:breeze")
    }

    fn make_brain(&mut self, packed: &BrainPacked) -> Brain {
        let mut brain = BRAIN_PROVIDER.make_brain(self, packed);
        brain.set_default_activity(Activity::FIGHT);
        brain.use_default_activity();
        brain
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        // client-only particles aside, the draws of `Breeze.tick()` run on the server too
        if matches!(self.base().get_pose(), Pose::Shooting | Pose::Inhaling | Pose::Standing) {
            // `emitGroundParticles(1 + this.getRandom().nextInt(1))`
            let _ = self.base_mut().random.next_int_bounded(1);
        }
        self.sound_tick = if self.sound_tick == 0 { self.base_mut().random.next_int_between_inclusive(WHIRL_SOUND_FREQUENCY_MIN, WHIRL_SOUND_FREQUENCY_MAX) } else { self.sound_tick - 1 };
        if self.sound_tick == 0 {
            // `playWhirlSound()`: a local sound (no-op on the server) with two random draws
            let _ = self.base_mut().random.next_float();
            let _ = self.base_mut().random.next_float();
        }
        mob::tick::tick(self, level);
    }

    fn play_ambient_sound(&mut self, _level: &mut ServerLevel) {
        // `level.playLocalSound(..)`: client only
    }

    fn deflection(&self, projectile: &dyn Projectile) -> ProjectileDeflection {
        let ty = projectile.get_type();
        if ty != EntityType::BREEZE_WIND_CHARGE && ty != EntityType::WIND_CHARGE {
            if mc_data::tags::get::<EntityType>("deflects_projectiles").is_some_and(|t| t.contains(self.get_type())) { ProjectileDeflection::Reverse } else { ProjectileDeflection::None }
        } else {
            ProjectileDeflection::None
        }
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_BREEZE_DEATH)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_BREEZE_HURT)
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(if self.base().on_ground() { SoundEvent::ENTITY_BREEZE_IDLE_GROUND } else { SoundEvent::ENTITY_BREEZE_IDLE_AIR })
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        Brain::tick(level, self);
        breeze_ai::update_activity(self);
    }

    fn can_attack(&self, level: &ServerLevel, target: &dyn Entity) -> bool {
        (target.get_type() == EntityType::PLAYER || target.get_type() == EntityType::IRON_GOLEM)
            && !(target.is_player() && level.difficulty() == mc_world::level_storage::Difficulty::Peaceful)
            && target.can_be_seen_as_enemy()
    }

    fn get_max_head_y_rot(&self) -> i32 {
        30
    }

    fn get_head_rot_speed(&self) -> i32 {
        25
    }

    fn is_invulnerable_to(&mut self, level: &mut ServerLevel, source: &DamageSource) -> bool {
        source.causing_info().is_some_and(|c| c.entity_type == EntityType::BREEZE) || damage::is_invulnerable_to(self, level, source)
    }

    fn get_fluid_jump_threshold(&self) -> f64 {
        f64::from(self.base().eye_height)
    }

    fn cause_fall_damage(&mut self, level: &mut ServerLevel, fall_distance: f64, damage_modifier: f32, source: &DamageSource) -> bool {
        if fall_distance > FALL_DISTANCE_SOUND_TRIGGER_THRESHOLD {
            self.play_sound(level, SoundEvent::ENTITY_BREEZE_LAND, 1.0, 1.0);
        }
        damage::cause_fall_damage(self, level, fall_distance, damage_modifier, source)
    }

    fn get_movement_emission(&self) -> MovementEmission {
        MovementEmission::Events
    }

    fn get_target(&self) -> Option<EntityId> {
        self.brain().get_memory(&crate::world::entity::ai::memory::memory_module_type::ATTACK_TARGET).map(|r| r.0)
    }

    fn get_sound_source(&self) -> mc_protocol::packets::play::SoundSource {
        mob::monster::SOUND_SOURCE
    }

    fn get_swim_sound(&self) -> SoundEvent {
        mob::monster::SWIM_SOUND
    }

    fn get_swim_splash_sound(&self) -> SoundEvent {
        mob::monster::SWIM_SPLASH_SOUND
    }

    fn get_fall_sounds(&self) -> (SoundEvent, SoundEvent) {
        mob::monster::FALL_SOUNDS
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static BREEZE: EntityRegistration = EntityRegistration { entity_type: EntityType::BREEZE, create: |ty, id, registries| Box::new(Breeze::new(ty, id, registries)) };

