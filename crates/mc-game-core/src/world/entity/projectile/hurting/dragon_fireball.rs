//! `net.minecraft.world.entity.projectile.hurtingprojectile.DragonFireball` (ender dragon breath ball: a lingering cloud).

use super::fireball::{hurting_data_impl, hurting_entity_impl, hurting_projectile_impl};
use super::{AbstractHurtingProjectile, HurtingProjectileData};
use crate::server::level::ServerLevel;
use crate::world::entity::area_effect_cloud::AreaEffectCloud;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::projectile::{HitResult, HitType, Projectile, ProjectileData};
use crate::world::entity::{Entity, EntityData, EntityId, entity};
use crate::world::level::LevelAccessor;
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_data::{EntityType, MobEffect, ParticleType};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::ParticleOptions;

pub const SPLASH_RANGE: f32 = 4.0;

/// `DragonFireball`.
pub struct DragonFireball {
    base: EntityData,
    projectile: ProjectileData,
    hurting: HurtingProjectileData,
}

impl DragonFireball {
    /// `DragonFireball(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        Self {
            base: EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |_| {}),
            projectile: ProjectileData::default(),
            hurting: HurtingProjectileData::default(),
        }
    }

    /// `DragonFireball(level, mob, direction)`.
    pub fn from_mob(level: &mut ServerLevel, mob: &dyn Entity, direction: Vec3) -> Self {
        let mut fireball = Self::new(EntityType::DRAGON_FIREBALL, level.next_entity_id());
        let b = mob.base();
        super::init_with_direction(&mut fireball, b.x(), b.y(), b.z(), direction);
        fireball.set_owner(Some(mob));
        fireball.set_rot(b.y_rot(), b.x_rot());
        fireball
    }
}

impl Projectile for DragonFireball {
    hurting_projectile_impl!();

    fn on_hit(&mut self, level: &mut ServerLevel, hit: &HitResult) {
        crate::world::entity::projectile::on_hit_base(self, level, hit);
        let owned_by_hit_entity = match hit {
            HitResult::Entity(e) if hit.get_type() == HitType::Entity => {
                level.get_entity(e.entity).is_some_and(|t| self.owned_by(t))
            }
            _ => false,
        };
        if hit.get_type() != HitType::Entity || !owned_by_hit_entity {
            let bb = self.get_bounding_box().inflate(4.0, 2.0, 4.0);
            let living = level.get_entities_of_class(&bb, |e| e.living().is_some(), |_| true);
            let mut cloud = AreaEffectCloud::at(level, self.base.x(), self.base.y(), self.base.z());
            if let Some(owner) = self.get_owner(level).filter(|o| o.living().is_some()) {
                cloud.set_owner(Some(owner));
            }
            cloud.set_custom_particle(Some(ParticleOptions::Power {
                particle_type: ParticleType::DRAGON_BREATH,
                power: 1.0,
            }));
            cloud.set_radius(3.0);
            cloud.set_duration(600);
            cloud.set_radius_per_tick((7.0 - cloud.get_radius()) / cloud.get_duration() as f32);
            cloud.set_potion_duration_scale(0.25);
            cloud.add_effect(&MobEffectInstance::of(MobEffect::INSTANT_DAMAGE, 1, 1));
            for id in living {
                let Some(entity) = level.get_entity(id) else {
                    continue;
                };
                let dist = self.base.distance_to_sqr_vec(entity.position());
                if dist < 16.0 {
                    let p = entity.position();
                    cloud.set_pos(p.x, p.y, p.z);
                    break;
                }
            }
            let pos = self.block_position();
            level.level_event(None, 2006, pos, if self.base.is_silent() { -1 } else { 1 });
            level.add_fresh_entity(Box::new(cloud));
            self.discard();
        }
    }
}

impl AbstractHurtingProjectile for DragonFireball {
    hurting_data_impl!();

    fn should_burn(&self) -> bool {
        false
    }
}

impl Entity for DragonFireball {
    hurting_entity_impl!();

    fn tick(&mut self, level: &mut ServerLevel) {
        super::tick(self, level);
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        super::add_additional_save_data(self, out);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        super::read_additional_save_data(self, input);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static DRAGON_FIREBALL_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::DRAGON_FIREBALL,
    create: |ty, id, _registries| Box::new(DragonFireball::new(ty, id)),
};
