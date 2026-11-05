//! `net.minecraft.world.entity.projectile.hurtingprojectile.windcharge`: `AbstractWindCharge`, `WindCharge` (player /
//! dispenser) and `BreezeWindCharge`.

use super::{AbstractHurtingProjectile, HurtingProjectileData};
use crate::server::level::ServerLevel;
use crate::server::level::level_explode::Explode;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::projectile::{EntityHitResult, HitResult, Projectile, ProjectileData, ProjectileDeflection};
use crate::world::entity::{Entity, EntityData, EntityId, entity};
use crate::world::item::ItemStack;
use crate::world::item::enchantment::actors::Actors;
use crate::world::level::{BlockGetter as _, LevelReader as _};
use crate::world::level::explosion::{ExplosionDamageCalculator, ExplosionInteraction, ImmuneBlocks, SimpleExplosionDamageCalculator};
use linkme::distributed_slice;
use mc_core::phys::{AABB, BlockHitResult, Vec3};
use mc_data::{EntityType, ParticleType, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::{ParticleOptions, SoundEventHolder, WeightedList};
use std::any::Any;
use uuid::Uuid;

pub const JUMP_SCALE: f64 = 0.25;
const DEFAULT_NO_DEFLECT_TICKS: i32 = 5;

const BLOCKS_WIND_CHARGE_EXPLOSIONS: ImmuneBlocks = ImmuneBlocks::Tag("blocks_wind_charge_explosions");

/// `AbstractWindCharge.EXPLOSION_DAMAGE_CALCULATOR` (the breeze's).
pub static EXPLOSION_DAMAGE_CALCULATOR: SimpleExplosionDamageCalculator =
    SimpleExplosionDamageCalculator { explodes_blocks: true, damages_entities: false, knockback_multiplier: None, immune_blocks: Some(BLOCKS_WIND_CHARGE_EXPLOSIONS) };

/// `WindCharge.EXPLOSION_DAMAGE_CALCULATOR` (knockback multiplier 1.22).
pub static WIND_CHARGE_EXPLOSION_DAMAGE_CALCULATOR: SimpleExplosionDamageCalculator =
    SimpleExplosionDamageCalculator { explodes_blocks: true, damages_entities: false, knockback_multiplier: Some(1.22), immune_blocks: Some(BLOCKS_WIND_CHARGE_EXPLOSIONS) };

/// `AbstractWindCharge`.
pub trait AbstractWindCharge: AbstractHurtingProjectile {
    /// `explode(position)`.
    fn explode(&mut self, level: &mut ServerLevel, position: Vec3);
    fn as_wind_charge_mut(&mut self) -> &mut dyn AbstractWindCharge;
}

/// `instanceof AbstractWindCharge`.
fn is_wind_charge(ty: EntityType) -> bool {
    ty == EntityType::WIND_CHARGE || ty == EntityType::BREEZE_WIND_CHARGE
}

/// `AbstractWindCharge.makeBoundingBox(position)`.
fn make_bounding_box(dimensions: &crate::world::entity::EntityDimensions, position: Vec3) -> AABB {
    let width = dimensions.width / 2.0;
    let height = dimensions.height;
    let offset = f64::from(0.15f32);
    AABB::new(position.x - f64::from(width), position.y - offset, position.z - f64::from(width), position.x + f64::from(width), position.y - offset + f64::from(height), position.z + f64::from(width))
}

/// `AbstractWindCharge.canHitEntity(entity)`.
fn can_hit_entity(wind_charge: &dyn AbstractWindCharge, level: &ServerLevel, entity: &dyn Entity) -> bool {
    if is_wind_charge(entity.get_type()) || entity.get_type() == EntityType::END_CRYSTAL {
        return false;
    }
    super::can_hit_entity(wind_charge, level, entity)
}

/// `AbstractWindCharge.onHitEntity(hitResult)`.
fn on_hit_entity(wind_charge: &mut dyn AbstractWindCharge, level: &mut ServerLevel, hit: &EntityHitResult) {
    let victim = hit.entity;
    let owner_id = wind_charge.get_owner(level).filter(|o| o.living().is_some()).map(|o| o.id());
    if let Some(owner) = owner_id {
        level.with_entity(owner, |level, o| {
            if let Some(target) = level.get_entity(victim) {
                o.set_last_hurt_mob(target);
            }
        });
    }
    let source = {
        let owner = owner_id.and_then(|id| level.get_entity(id));
        level.damage_sources().wind_charge(wind_charge.as_dyn(), owner)
    };
    level.with_entity(victim, |level, target| {
        if target.hurt_server(level, &source, 1.0) && target.living().is_some() {
            crate::world::item::enchantment_helper::do_post_attack_effects(level, &mut Actors::of(&mut *target).and(wind_charge.as_dyn_mut()), victim, &source);
        }
    });
    let position = wind_charge.position();
    wind_charge.explode(level, position);
}

/// `AbstractWindCharge.onHitBlock(hitResult)`.
fn on_hit_block(wind_charge: &mut dyn AbstractWindCharge, level: &mut ServerLevel, hit: &BlockHitResult) {
    crate::world::entity::projectile::on_hit_block_base(wind_charge, level, hit);
    let n = hit.direction;
    let scaled_normal = Vec3::new(f64::from(n.get_step_x()), f64::from(n.get_step_y()), f64::from(n.get_step_z())).multiply_xyz(0.25, 0.25, 0.25);
    let explosion_pos = hit.location.add(scaled_normal);
    wind_charge.explode(level, explosion_pos);
    wind_charge.discard();
}

/// `AbstractWindCharge.onHit(hitResult)`.
fn on_hit(wind_charge: &mut dyn AbstractWindCharge, level: &mut ServerLevel, hit: &HitResult) {
    crate::world::entity::projectile::on_hit_base(wind_charge, level, hit);
    wind_charge.discard();
}

/// `AbstractWindCharge.tick()`.
fn tick(wind_charge: &mut dyn AbstractWindCharge, level: &mut ServerLevel) {
    if wind_charge.block_position().y() > level.get_max_y() + 30 {
        let position = wind_charge.position();
        wind_charge.explode(level, position);
        wind_charge.discard();
    } else {
        super::tick(wind_charge, level);
    }
}

/// `level.explode(this, null, calculator, x, y, z, radius, false, TRIGGER, GUST_EMITTER_SMALL, GUST_EMITTER_LARGE, WeightedList.of(), sound)`.
fn explode_with(wind_charge: &dyn Entity, level: &mut ServerLevel, calculator: &dyn ExplosionDamageCalculator, position: Vec3, radius: f32, sound: SoundEvent) {
    let mut params = Explode::new(Some(wind_charge), position.x, position.y, position.z, radius, ExplosionInteraction::Trigger).with_damage_source(None, Some(calculator));
    params.small_explosion_particles = ParticleOptions::Simple(ParticleType::GUST_EMITTER_SMALL);
    params.large_explosion_particles = ParticleOptions::Simple(ParticleType::GUST_EMITTER_LARGE);
    params.block_particles = WeightedList(Vec::new());
    params.explosion_sound = SoundEventHolder::Reference(sound);
    level.explode(params, &mut []);
}

/// The `AbstractWindCharge` overrides shared by both classes, for the `Projectile` impl.
macro_rules! wind_charge_projectile {
    () => {
        fn projectile_data(&self) -> &ProjectileData {
            &self.projectile
        }
        fn projectile_data_mut(&mut self) -> &mut ProjectileData {
            &mut self.projectile
        }
        fn can_hit_entity(&self, level: &ServerLevel, entity: &dyn Entity) -> bool {
            can_hit_entity(self, level, entity)
        }
        fn on_deflection(&mut self, _level: &mut ServerLevel, by_attack: bool) {
            super::on_deflection(self, by_attack);
        }
        fn on_hit_entity(&mut self, level: &mut ServerLevel, hit: &EntityHitResult) {
            on_hit_entity(self, level, hit);
        }
        fn on_hit_block(&mut self, level: &mut ServerLevel, hit: &BlockHitResult) {
            on_hit_block(self, level, hit);
        }
        fn on_hit(&mut self, level: &mut ServerLevel, hit: &HitResult) {
            on_hit(self, level, hit);
        }
    };
}

/// The `AbstractHurtingProjectile`/`AbstractWindCharge` overrides shared by both classes.
macro_rules! wind_charge_hurting {
    () => {
        fn hurting_data(&self) -> &HurtingProjectileData {
            &self.hurting
        }
        fn hurting_data_mut(&mut self) -> &mut HurtingProjectileData {
            &mut self.hurting
        }
        fn as_hurting_mut(&mut self) -> &mut dyn AbstractHurtingProjectile {
            self
        }
        fn should_burn(&self) -> bool {
            false
        }
        fn get_inertia(&self) -> f32 {
            1.0
        }
        fn get_liquid_inertia(&self) -> f32 {
            1.0
        }
    };
}

/// The `Entity` overrides shared by both classes.
macro_rules! wind_charge_entity {
    () => {
        fn base(&self) -> &EntityData {
            &self.base
        }
        fn base_mut(&mut self) -> &mut EntityData {
            &mut self.base
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
        fn as_dyn(&self) -> &dyn Entity {
            self
        }
        fn as_dyn_mut(&mut self) -> &mut dyn Entity {
            self
        }
        crate::impl_projectile_entity!(hurting);
        fn hurt_server(&mut self, _level: &mut ServerLevel, _source: &crate::world::damagesource::DamageSource, _amount: f32) -> bool {
            false
        }
        fn make_bounding_box(&self, position: Vec3) -> AABB {
            make_bounding_box(&self.base.dimensions, position)
        }
        fn can_collide_with(&self, level: &ServerLevel, entity: &dyn Entity) -> bool {
            !is_wind_charge(entity.get_type()) && entity.can_be_collided_with(Some(self.as_dyn())) && !self.is_passenger_of_same_vehicle(level, entity)
        }
        fn push_xyz(&mut self, _xa: f64, _ya: f64, _za: f64) {}
        fn add_additional_save_data(&self, out: &mut CompoundTag) {
            super::add_additional_save_data(self, out);
        }
        fn read_additional_save_data(&mut self, input: &CompoundTag) {
            super::read_additional_save_data(self, input);
        }
    };
}

/// `WindCharge`: thrown by players and dispensers.
pub struct WindCharge {
    base: EntityData,
    projectile: ProjectileData,
    hurting: HurtingProjectileData,
    no_deflect_ticks: i32,
}

impl WindCharge {
    /// `WindCharge(type, level)` (`accelerationPower = 0`).
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let base = EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |_| {});
        let mut hurting = HurtingProjectileData::default();
        hurting.acceleration_power = 0.0;
        Self { base, projectile: ProjectileData::default(), hurting, no_deflect_ticks: DEFAULT_NO_DEFLECT_TICKS }
    }

    /// `WindCharge(player, level, x, y, z)`: `AbstractWindCharge(type, level, owner, x, y, z)`.
    pub fn from_player(level: &mut ServerLevel, player: &dyn Entity, x: f64, y: f64, z: f64) -> Self {
        let mut charge = Self::new(EntityType::WIND_CHARGE, level.next_entity_id());
        charge.set_pos(x, y, z);
        charge.set_owner(Some(player));
        charge
    }

    /// `WindCharge(level, x, y, z, direction)`: moves along `direction.normalize() * 0.1` (the hurting projectile's
    /// initial acceleration power), then the power is zeroed.
    pub fn with_direction(level: &mut ServerLevel, x: f64, y: f64, z: f64, direction: Vec3) -> Self {
        let mut charge = Self::new(EntityType::WIND_CHARGE, level.next_entity_id());
        charge.hurting.acceleration_power = super::INITIAL_ACCELERATION_POWER;
        super::init_with_direction(&mut charge, x, y, z, direction);
        charge.hurting.acceleration_power = 0.0;
        charge
    }
}

impl Projectile for WindCharge {
    wind_charge_projectile!();
    fn deflect(&mut self, level: &mut ServerLevel, deflection: ProjectileDeflection, deflecting_entity: Option<EntityId>, new_owner: Option<Uuid>, by_attack: bool, power: Vec3) -> bool {
        if self.no_deflect_ticks > 0 {
            false
        } else {
            crate::world::entity::projectile::deflect_base(self, level, deflection, deflecting_entity, new_owner, by_attack, power)
        }
    }
}

impl AbstractHurtingProjectile for WindCharge {
    wind_charge_hurting!();
}

impl AbstractWindCharge for WindCharge {
    fn explode(&mut self, level: &mut ServerLevel, position: Vec3) {
        explode_with(self, level, &WIND_CHARGE_EXPLOSION_DAMAGE_CALCULATOR, position, 1.2, SoundEvent::ENTITY_WIND_CHARGE_WIND_BURST);
    }
    fn as_wind_charge_mut(&mut self) -> &mut dyn AbstractWindCharge {
        self
    }
}

impl Entity for WindCharge {
    wind_charge_entity!();
    fn tick(&mut self, level: &mut ServerLevel) {
        tick(self, level);
        if self.no_deflect_ticks > 0 {
            self.no_deflect_ticks -= 1;
        }
    }
}

/// `BreezeWindCharge`: shot by the breeze.
pub struct BreezeWindCharge {
    base: EntityData,
    projectile: ProjectileData,
    hurting: HurtingProjectileData,
}

impl BreezeWindCharge {
    /// `BreezeWindCharge(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let base = EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |_| {});
        let mut hurting = HurtingProjectileData::default();
        hurting.acceleration_power = 0.0;
        Self { base, projectile: ProjectileData::default(), hurting }
    }

    /// `BreezeWindCharge(breeze, level)`: at `(breeze.x, breeze.getFiringYPosition(), breeze.z)`.
    pub fn from_breeze(level: &mut ServerLevel, breeze: &dyn Entity, firing_y_position: f64) -> Self {
        let mut charge = Self::new(EntityType::BREEZE_WIND_CHARGE, level.next_entity_id());
        charge.set_pos(breeze.base().x(), firing_y_position, breeze.base().z());
        charge.set_owner(Some(breeze));
        charge
    }
}

impl Projectile for BreezeWindCharge {
    wind_charge_projectile!();
}

impl AbstractHurtingProjectile for BreezeWindCharge {
    wind_charge_hurting!();
}

impl AbstractWindCharge for BreezeWindCharge {
    fn explode(&mut self, level: &mut ServerLevel, position: Vec3) {
        explode_with(self, level, &EXPLOSION_DAMAGE_CALCULATOR, position, 3.0, SoundEvent::ENTITY_BREEZE_WIND_BURST);
    }
    fn as_wind_charge_mut(&mut self) -> &mut dyn AbstractWindCharge {
        self
    }
}

impl Entity for BreezeWindCharge {
    wind_charge_entity!();
    fn tick(&mut self, level: &mut ServerLevel) {
        tick(self, level);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static WIND_CHARGE_FACTORY: EntityRegistration = EntityRegistration { entity_type: EntityType::WIND_CHARGE, create: |ty, level| Box::new(WindCharge::new(ty, level.next_entity_id())) };

#[distributed_slice(ENTITY_FACTORIES)]
static BREEZE_WIND_CHARGE_FACTORY: EntityRegistration =
    EntityRegistration { entity_type: EntityType::BREEZE_WIND_CHARGE, create: |ty, level| Box::new(BreezeWindCharge::new(ty, level.next_entity_id())) };
