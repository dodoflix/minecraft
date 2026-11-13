//! `net.minecraft.world.entity.projectile.hurtingprojectile.WitherSkull`.

use super::fireball::{hurting_data_impl, hurting_entity_impl, hurting_projectile_impl};
use super::{AbstractHurtingProjectile, HurtingProjectileData};
use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::server::level::level_explode::Explode;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::projectile::{EntityHitResult, HitResult, Projectile, ProjectileData};
use crate::world::entity::{Entity, EntityData, EntityId, entity};
use crate::world::item::enchantment::actors::Actors;
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt as _;
use crate::world::level::explosion::{Explosion, ExplosionInteraction};
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::fluid::FluidState;
use mc_data::{EntityType, MobEffect};
use mc_nbt::CompoundTag;
use mc_world::level_storage::Difficulty;

define_accessor!(pub DATA_DANGEROUS: Boolean = WitherSkull);

/// `WitherBoss.canDestroy(block)`: `!block.isAir() && !block.is(BlockTags.WITHER_IMMUNE)`.
pub fn can_destroy(state: BlockState) -> bool {
    !state.is_air() && !state.is_tag("wither_immune")
}

/// `WitherSkull`.
pub struct WitherSkull {
    base: EntityData,
    projectile: ProjectileData,
    hurting: HurtingProjectileData,
}

impl WitherSkull {
    /// `WitherSkull(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let base = EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_DANGEROUS, false);
        });
        Self {
            base,
            projectile: ProjectileData::default(),
            hurting: HurtingProjectileData::default(),
        }
    }

    /// `WitherSkull(level, mob, direction)`.
    pub fn from_mob(level: &mut ServerLevel, mob: &dyn Entity, direction: Vec3) -> Self {
        let mut skull = Self::new(EntityType::WITHER_SKULL, level.next_entity_id());
        let b = mob.base();
        super::init_with_direction(&mut skull, b.x(), b.y(), b.z(), direction);
        skull.set_owner(Some(mob));
        skull.set_rot(b.y_rot(), b.x_rot());
        skull
    }

    /// `isDangerous()`.
    pub fn is_dangerous(&self) -> bool {
        self.base.entity_data.get(DATA_DANGEROUS)
    }

    /// `setDangerous(value)`.
    pub fn set_dangerous(&mut self, value: bool) {
        self.base.entity_data.set(DATA_DANGEROUS, value);
    }
}

impl Projectile for WitherSkull {
    hurting_projectile_impl!();

    fn on_hit(&mut self, level: &mut ServerLevel, hit: &HitResult) {
        crate::world::entity::projectile::on_hit_base(self, level, hit);
        let p = self.position();
        level.explode(Explode::new(
            Some(self.as_dyn_mut()),
            p.x,
            p.y,
            p.z,
            1.0,
            ExplosionInteraction::Mob,
        ));
        self.discard_in_level(level);
    }

    fn on_hit_entity(&mut self, level: &mut ServerLevel, hit: &EntityHitResult) {
        let victim = hit.entity;
        let owner_id = self
            .get_owner(level)
            .filter(|o| o.living().is_some())
            .map(|o| o.id());
        let effect_source = self.get_effect_source(level);
        let source = match owner_id.and_then(|id| level.get_entity(id)) {
            Some(owner) => level.damage_sources().wither_skull(self.as_dyn(), owner),
            None => level.damage_sources().magic(),
        };
        let was_hurt = level
            .with_entity(victim, |level, target| {
                let was_hurt = level.lend(self.as_dyn_mut(), |level| {
                    target.hurt_server(level, &source, if owner_id.is_some() { 8.0 } else { 5.0 })
                });
                let mut owner_heal = false;
                if was_hurt && owner_id.is_some() {
                    if target.is_alive() {
                        crate::world::item::enchantment_helper::do_post_attack_effects(
                            level,
                            &mut Actors::of(&mut *target),
                            victim,
                            &source,
                        );
                    } else {
                        owner_heal = true;
                    }
                }
                (was_hurt, owner_heal, target.living().is_some())
            })
            .unwrap_or((false, false, false));
        if let (true, true, _) = was_hurt
            && let Some(owner) = owner_id
        {
            level.with_entity(owner, |level, o| o.heal(level, 5.0));
        }
        if was_hurt.0 && was_hurt.2 {
            let wither_seconds = match level.difficulty() {
                Difficulty::Normal => 10,
                Difficulty::Hard => 40,
                _ => 0,
            };
            if wither_seconds > 0 {
                level.with_entity(victim, |level, target| {
                    target.add_effect(
                        level,
                        MobEffectInstance::of(MobEffect::WITHER, 20 * wither_seconds, 1),
                        Some(effect_source),
                    );
                });
            }
        }
    }
}

impl AbstractHurtingProjectile for WitherSkull {
    hurting_data_impl!();

    fn get_inertia(&self) -> f32 {
        if self.is_dangerous() { 0.73 } else { 0.95 }
    }

    fn should_burn(&self) -> bool {
        false
    }
}

impl Entity for WitherSkull {
    hurting_entity_impl!();

    fn tick(&mut self, level: &mut ServerLevel) {
        super::tick(self, level);
    }

    fn is_on_fire(&self) -> bool {
        false
    }

    fn get_block_explosion_resistance(
        &self,
        _explosion: &dyn Explosion,
        _level: &dyn BlockGetter,
        _pos: BlockPos,
        block: BlockState,
        _fluid: FluidState,
        resistance: f32,
    ) -> f32 {
        if self.is_dangerous() && can_destroy(block) {
            resistance.min(0.8)
        } else {
            resistance
        }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        super::add_additional_save_data(self, out);
        out.put_boolean("dangerous", self.is_dangerous());
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        super::read_additional_save_data(self, input);
        self.set_dangerous(input.get_boolean_or("dangerous", false));
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static WITHER_SKULL_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::WITHER_SKULL,
    create: |ty, id, _registries| Box::new(WitherSkull::new(ty, id)),
};
