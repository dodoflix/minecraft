//! `net.minecraft.world.entity.animal.squid.Squid` (+ `SquidRandomMovementGoal`, `SquidFleeGoal`).
//!
//! `GlowSquid extends Squid`: both embed [`SquidData`] (the fields of the Java `Squid` class) and share the behavior
//! through the free functions of this module; [`as_squid_data`] is the `instanceof Squid` of the goals.

use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::ai::goal::{Goal, GoalFlags, reduced_tick_delay};
use crate::world::entity::ai::navigation::StandardPathNavigation;
use crate::world::entity::dimensions::EntityDimensions;
use crate::world::entity::factory::{self, ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest};
use crate::world::entity::living_entity::damage::make_sound;
use crate::world::entity::living_entity::util::{broadcast_entity_event, l};
use crate::world::entity::mob::spawn::AgeableMobGroupData;
use crate::world::entity::mob::{self, MobData, MobKind};
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::entity::{Entity, EntityId, MoverType};
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{EntityType, MobEffect, SoundEvent};
use mc_protocol::packets::play::ParticleOptions;

const PI_F: f32 = std::f32::consts::PI;
const DEG_TO_RAD_F: f32 = (std::f64::consts::PI / 180.0) as f32;
const TWO_PI_F: f32 = (std::f64::consts::PI * 2.0) as f32;

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    EntityDimensions::scalable(0.5, 0.5).with_eye_height(0.37)
}

/// The fields of the Java class `Squid`.
#[derive(Debug)]
pub struct SquidData {
    pub x_body_rot: f32,
    pub x_body_rot_o: f32,
    pub z_body_rot: f32,
    pub z_body_rot_o: f32,
    pub tentacle_movement: f32,
    pub old_tentacle_movement: f32,
    pub tentacle_angle: f32,
    pub old_tentacle_angle: f32,
    speed: f32,
    tentacle_speed: f32,
    rotate_speed: f32,
    pub movement_vector: Vec3,
}

impl SquidData {
    /// The field initializers + the constructor statement `tentacleSpeed = 1.0F / (random.nextFloat() + 1.0F) * 0.2F`.
    pub fn new(mob: &mut MobData) -> SquidData {
        use mc_core::random::RandomSource;
        let tentacle_speed = 1.0 / (mob.living.entity.random.next_float() + 1.0) * 0.2;
        SquidData {
            x_body_rot: 0.0,
            x_body_rot_o: 0.0,
            z_body_rot: 0.0,
            z_body_rot_o: 0.0,
            tentacle_movement: 0.0,
            old_tentacle_movement: 0.0,
            tentacle_angle: 0.0,
            old_tentacle_angle: 0.0,
            speed: 0.0,
            tentacle_speed,
            rotate_speed: 0.0,
            movement_vector: Vec3::ZERO,
        }
    }

    /// `hasMovementVector()`.
    pub fn has_movement_vector(&self) -> bool {
        self.movement_vector.length_sqr() > f64::from(1.0E-5f32)
    }
}

/// The members a `Squid` subclass supplies.
pub trait SquidLike: Entity {
    fn squid(&self) -> &SquidData;
    fn squid_mut(&mut self) -> &mut SquidData;
    /// `getSquirtSound()`.
    fn get_squirt_sound(&self) -> SoundEvent {
        SoundEvent::ENTITY_SQUID_SQUIRT
    }
    /// `getInkParticle()`.
    fn get_ink_particle(&self) -> ParticleOptions {
        ParticleOptions::Simple(mc_data::ParticleType::SQUID_INK)
    }
}

/// `entity instanceof Squid`.
pub fn as_squid_data(e: &mut dyn Entity) -> Option<&mut SquidData> {
    let any = e.as_any_mut();
    if any.is::<Squid>() {
        return any.downcast_mut::<Squid>().map(|s| &mut s.squid);
    }
    any.downcast_mut::<super::glow_squid::GlowSquid>().map(|s| &mut s.squid)
}

/// `squid` field shared read access for the goals (`instanceof Squid`).
pub fn as_squid_data_ref(e: &dyn Entity) -> Option<&SquidData> {
    let any = e.as_any();
    if let Some(s) = any.downcast_ref::<Squid>() {
        return Some(&s.squid);
    }
    any.downcast_ref::<super::glow_squid::GlowSquid>().map(|s| &s.squid)
}

/// `Mob.<init>` tail shared by squids: ground navigation, goals, water malus (`AgeableWaterCreature.<init>`).
pub fn init(mob: &mut MobData, navigation: StandardPathNavigation) {
    mob.set_pathfinding_malus(crate::world::level::pathfinder::PathType::Water, 0.0);
    mob.set_navigation(Box::new(navigation));
    mob.goal_selector.add_goal(0, Box::new(SquidRandomMovementGoal));
    mob.goal_selector.add_goal(1, Box::new(SquidFleeGoal { flee_ticks: 0 }));
}

/// `rotateVector(vec)`.
fn rotate_vector(e: &dyn Entity, squid: &SquidData, vec: Vec3) -> Vec3 {
    let v = vec.x_rot(squid.x_body_rot_o * DEG_TO_RAD_F);
    v.y_rot(-l(e).y_body_rot_o * DEG_TO_RAD_F)
}

/// `spawnInk()`.
fn spawn_ink<E: SquidLike>(e: &mut E, level: &mut ServerLevel) {
    use mc_core::random::RandomSource;
    let sound = e.get_squirt_sound();
    make_sound(e, level, sound);
    let pos = rotate_vector(e, e.squid(), Vec3::new(0.0, -1.0, 0.0)).add(Vec3::new(e.base().x(), e.base().y(), e.base().z()));
    let particle = e.get_ink_particle();
    for _ in 0..30 {
        let rx = e.base_mut().random.next_float();
        let rz = e.base_mut().random.next_float();
        let dir = rotate_vector(e, e.squid(), Vec3::new(f64::from(rx) * 0.6 - 0.3, -1.0, f64::from(rz) * 0.6 - 0.3));
        let ink_pos_offset_scale = if e.is_baby() { 0.1f32 } else { 0.3f32 };
        let scale = f64::from(ink_pos_offset_scale + e.base_mut().random.next_float() * 2.0);
        let dir_offset = dir.scale(scale);
        level.send_particles(particle.clone(), pos.x, pos.y + 0.5, pos.z, 0, dir_offset.x, dir_offset.y, dir_offset.z, f64::from(0.1f32));
    }
}

/// `Squid.aiStep()`.
pub fn ai_step<E: SquidLike>(e: &mut E, level: &mut ServerLevel) {
    use mc_core::random::RandomSource;
    mob::tick::ai_step(e, level);
    {
        let s = e.squid_mut();
        s.x_body_rot_o = s.x_body_rot;
        s.z_body_rot_o = s.z_body_rot;
        s.old_tentacle_movement = s.tentacle_movement;
        s.old_tentacle_angle = s.tentacle_angle;
        s.tentacle_movement += s.tentacle_speed;
    }
    if f64::from(e.squid().tentacle_movement) > std::f64::consts::PI * 2.0 {
        e.squid_mut().tentacle_movement -= TWO_PI_F;
        if e.base_mut().random.next_int_bounded(10) == 0 {
            let r = e.base_mut().random.next_float();
            e.squid_mut().tentacle_speed = 1.0 / (r + 1.0) * 0.2;
        }
        broadcast_entity_event(level, e.id(), 19);
    }
    if e.is_in_water() {
        if e.squid().tentacle_movement < PI_F {
            let tentacle_scale = e.squid().tentacle_movement / PI_F;
            e.squid_mut().tentacle_angle = mth::sin(f64::from(tentacle_scale * tentacle_scale * PI_F)) * PI_F * 0.25;
            if f64::from(tentacle_scale) > 0.75 {
                if e.is_local_instance_authoritative() {
                    let mv = e.squid().movement_vector;
                    e.base_mut().set_delta_movement(mv);
                }
                e.squid_mut().rotate_speed = 1.0;
            } else {
                e.squid_mut().rotate_speed *= 0.8;
            }
        } else {
            e.squid_mut().tentacle_angle = 0.0;
            if e.is_local_instance_authoritative() {
                let dm = e.base().delta_movement().scale(0.9);
                e.base_mut().set_delta_movement(dm);
            }
            e.squid_mut().rotate_speed *= 0.99;
        }
        let movement = e.base().delta_movement();
        let horizontal_movement = movement.horizontal_distance();
        let y_body_rot = l(e).y_body_rot;
        let new_y_body_rot = y_body_rot + (-(mth::atan2(movement.x, movement.z) as f32) * (180.0 / PI_F) - y_body_rot) * 0.1;
        e.living_mut().expect("living").y_body_rot = new_y_body_rot;
        e.set_y_rot(new_y_body_rot);
        let s = e.squid_mut();
        s.z_body_rot += PI_F * s.rotate_speed * 1.5;
        s.x_body_rot += (-(mth::atan2(horizontal_movement, movement.y) as f32) * (180.0 / PI_F) - s.x_body_rot) * 0.1;
    } else {
        let s = e.squid_mut();
        s.tentacle_angle = mth::abs_f32(mth::sin(f64::from(s.tentacle_movement))) * PI_F * 0.25;
        let mut yd = e.base().delta_movement().y;
        if let Some(amplifier) = crate::world::entity::living_entity::util::effect_amplifier(e, MobEffect::LEVITATION) {
            yd = 0.05 * f64::from(amplifier + 1);
        } else {
            yd -= e.get_gravity();
        }
        let drag = f64::from(e.get_air_drag());
        e.base_mut().set_delta_movement(Vec3::new(0.0, yd * drag, 0.0));
        let s = e.squid_mut();
        s.x_body_rot += (-90.0 - s.x_body_rot) * 0.02;
    }
}

/// `getLastHurtByMob()`: the entity that last hurt `e`, resolved through the level (also while that entity is taken
/// out of its slot because it is the one running, e.g. the attacker of the hit being processed).
pub fn get_last_hurt_by_mob(level: &ServerLevel, e: &dyn Entity) -> Option<EntityId> {
    let uuid = l(e).last_hurt_by_mob?;
    level.entity_manager.id_by_uuid(uuid)
}

/// `Squid.hurtServer(level, source, damage)`.
pub fn hurt_server<E: SquidLike>(e: &mut E, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
    if crate::world::entity::living_entity::damage::hurt_server(e, level, source, damage) && get_last_hurt_by_mob(level, e).is_some() {
        spawn_ink(e, level);
        true
    } else {
        false
    }
}

/// `Squid.travel(input)`.
pub fn travel(e: &mut dyn Entity, level: &mut ServerLevel) {
    let dm = e.base().delta_movement();
    e.r#move(level, MoverType::SelfMover, dm);
}

/// `Squid.finalizeSpawn`: `requireNonNullElseGet(groupData, () -> new AgeableMobGroupData(0.05F))`.
pub fn finalize_spawn(
    e: &mut dyn Entity,
    level: &mut dyn ServerLevelAccessor,
    difficulty: DifficultyInstance,
    reason: EntitySpawnReason,
    group_data: Option<Box<dyn SpawnGroupData>>,
) -> Option<Box<dyn SpawnGroupData>> {
    let group_data = group_data.unwrap_or_else(|| Box::new(AgeableMobGroupData::with_chance(0.05)));
    mob::spawn::finalize_spawn(e, level, difficulty, reason, Some(group_data))
}

/// `getDefaultDimensions(pose)`.
pub fn get_dimensions(e: &dyn Entity, _pose: Pose) -> EntityDimensions {
    if e.is_baby() { baby_dimensions() } else { EntityDimensions::of_type(e.get_type()) }
}

/// `getBreedOffspring(level, partner)`: `EntityTypes.SQUID/GLOW_SQUID.create(level, BREEDING)`.
pub fn get_breed_offspring(ty: EntityType, level: &mut ServerLevel) -> Option<Box<dyn Entity>> {
    factory::create(ty, level, EntitySpawnRequest { reason: EntitySpawnReason::Breeding, ignore_checks: false })
}

/// The `Squid` overrides for an `impl Entity` block of `Squid` / `GlowSquid` (both write the rest themselves).
macro_rules! squid_entity {
    () => {
        crate::world::entity::animal::squid::squid::squid_entity!(skip_ai_step_hurt);
        fn ai_step(&mut self, level: &mut crate::server::level::ServerLevel) {
            crate::world::entity::animal::squid::squid::ai_step(self, level);
        }
        fn hurt_server(&mut self, level: &mut crate::server::level::ServerLevel, source: &crate::world::damagesource::DamageSource, damage: f32) -> bool {
            crate::world::entity::animal::squid::squid::hurt_server(self, level, source, damage)
        }
    };
    (skip_ai_step_hurt) => {
        fn can_be_leashed(&self) -> bool {
            true
        }
        fn get_sound_volume(&self) -> f32 {
            0.4
        }
        fn get_movement_emission(&self) -> crate::world::entity::entity::MovementEmission {
            crate::world::entity::entity::MovementEmission::Events
        }
        fn get_default_gravity(&self) -> f64 {
            0.08
        }
        fn travel(&mut self, level: &mut crate::server::level::ServerLevel, _input: mc_core::phys::Vec3) {
            crate::world::entity::animal::squid::squid::travel(self, level);
        }
        fn finalize_spawn(
            &mut self,
            level: &mut dyn crate::world::level::server_level_accessor::ServerLevelAccessor,
            difficulty: crate::world::difficulty_instance::DifficultyInstance,
            reason: mc_data::enums::EntitySpawnReason,
            group_data: Option<Box<dyn crate::world::entity::spawn_group_data::SpawnGroupData>>,
        ) -> Option<Box<dyn crate::world::entity::spawn_group_data::SpawnGroupData>> {
            crate::world::entity::animal::squid::squid::finalize_spawn(self, level, difficulty, reason, group_data)
        }
        fn get_dimensions(&self, pose: mc_data::enums::Pose) -> crate::world::entity::dimensions::EntityDimensions {
            crate::world::entity::animal::squid::squid::get_dimensions(self, pose)
        }
    };
}
pub(crate) use squid_entity;

/// `Squid`.
pub struct Squid {
    pub mob: MobData,
    pub squid: SquidData,
}

impl Squid {
    /// `new Squid(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mut mob = MobData::new(ty, id, MobKind::AgeableMob, |_| {});
        let squid = SquidData::new(&mut mob);
        let mut s = Squid { mob, squid };
        let navigation = StandardPathNavigation::ground(&s);
        init(&mut s.mob, navigation);
        s
    }
}

impl SquidLike for Squid {
    fn squid(&self) -> &SquidData {
        &self.squid
    }
    fn squid_mut(&mut self) -> &mut SquidData {
        &mut self.squid
    }
}

impl Entity for Squid {
    crate::mob_entity_accessors!(mob);
    crate::world::entity::animal::fish::water_animal::water_creature_entity!(no_leash);
    squid_entity!();

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:squid")
    }

    fn get_breed_offspring(&mut self, level: &mut ServerLevel, _partner: EntityId) -> Option<Box<dyn Entity>> {
        get_breed_offspring(EntityType::SQUID, level)
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SQUID_AMBIENT)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SQUID_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SQUID_DEATH)
    }
}

/// `Squid.SquidRandomMovementGoal`.
struct SquidRandomMovementGoal;

impl Goal for SquidRandomMovementGoal {
    fn can_use(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        use mc_core::random::RandomSource;
        let no_action_time = l(mob).no_action_time;
        if no_action_time > 100 {
            if let Some(s) = as_squid_data(mob) {
                s.movement_vector = Vec3::ZERO;
            }
        } else if mob.base_mut().random.next_int_bounded(reduced_tick_delay(50)) == 0
            || !mob.base().was_touching_water
            || !as_squid_data_ref(mob).is_some_and(|s| s.has_movement_vector())
        {
            let angle = mob.base_mut().random.next_float() * TWO_PI_F;
            let y = -0.1f32 + mob.base_mut().random.next_float() * 0.2;
            if let Some(s) = as_squid_data(mob) {
                s.movement_vector = Vec3::new(f64::from(mth::cos(f64::from(angle)) * 0.2), f64::from(y), f64::from(mth::sin(f64::from(angle)) * 0.2));
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}

/// `Squid.SquidFleeGoal`.
struct SquidFleeGoal {
    flee_ticks: i32,
}

impl Goal for SquidFleeGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let Some(entity) = get_last_hurt_by_mob(level, mob).and_then(|id| level.get_entity(id)) else { return false };
        mob.is_in_water() && mob.base().distance_to_sqr_vec(entity.position()) < 100.0
    }

    fn start(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.flee_ticks = 0;
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.flee_ticks += 1;
        let Some(last_hurt_by_mob) = get_last_hurt_by_mob(level, mob).and_then(|id| level.get_entity(id)).map(|e| e.position()) else { return };
        let (x, y, z) = (mob.base().x(), mob.base().y(), mob.base().z());
        let mut flee_to = Vec3::new(x - last_hurt_by_mob.x, y - last_hurt_by_mob.y, z - last_hurt_by_mob.z);
        let target = BlockPos::containing(x + flee_to.x, y + flee_to.y, z + flee_to.z);
        let block_state = level.get_block_state(target);
        let fluid_state = level.get_fluid_state(target);
        if fluid_state.is_tag("water") || block_state.is_air() {
            let length = flee_to.length();
            if length > 0.0 {
                // `fleeTo.normalize()` (result discarded in vanilla)
                let mut avoid_speed = 3.0;
                if length > 5.0 {
                    avoid_speed -= (length - 5.0) / 5.0;
                }
                if avoid_speed > 0.0 {
                    flee_to = flee_to.scale(avoid_speed);
                }
            }
            if block_state.is_air() {
                flee_to = flee_to.subtract(Vec3::new(0.0, flee_to.y, 0.0));
            }
            if let Some(s) = as_squid_data(mob) {
                s.movement_vector = Vec3::new(flee_to.x / 20.0, flee_to.y / 20.0, flee_to.z / 20.0);
            }
        }
        // `if (fleeTicks % 10 == 5) level.addParticle(BUBBLE, ..)`: client-side particle, no server effect
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static SQUID: EntityRegistration = EntityRegistration { entity_type: EntityType::SQUID, create: |ty, id, _registries| Box::new(Squid::new(ty, id)) };
