//! `net.minecraft.world.entity.projectile.hurtingprojectile.LargeFireball` (ghast fireball, `minecraft:fireball`).

use super::fireball::{
    self, DATA_ITEM_STACK, hurting_data_impl, hurting_entity_impl, hurting_projectile_impl,
};
use super::{AbstractHurtingProjectile, HurtingProjectileData};
use crate::server::level::ServerLevel;
use crate::server::level::level_explode::Explode;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::projectile::{EntityHitResult, HitResult, Projectile, ProjectileData};
use crate::world::entity::{Entity, EntityData, EntityId, entity};
use crate::world::item::ItemStack;
use crate::world::item::enchantment::actors::Actors;
use crate::world::level::explosion::ExplosionInteraction;
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_data::EntityType;
use mc_nbt::CompoundTag;

const DEFAULT_EXPLOSION_POWER: i32 = 1;

/// `LargeFireball`.
pub struct LargeFireball {
    base: EntityData,
    projectile: ProjectileData,
    hurting: HurtingProjectileData,
    /// `explosionPower`.
    pub explosion_power: i32,
}

impl LargeFireball {
    /// `LargeFireball(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let base = EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_ITEM_STACK, fireball::get_default_item());
        });
        Self {
            base,
            projectile: ProjectileData::default(),
            hurting: HurtingProjectileData::default(),
            explosion_power: DEFAULT_EXPLOSION_POWER,
        }
    }

    /// `LargeFireball(level, mob, direction, explosionPower)`.
    pub fn from_mob(
        level: &mut ServerLevel,
        mob: &dyn Entity,
        direction: Vec3,
        explosion_power: i32,
    ) -> Self {
        let mut fireball = Self::new(EntityType::FIREBALL, level.next_entity_id());
        let b = mob.base();
        super::init_with_direction(&mut fireball, b.x(), b.y(), b.z(), direction);
        fireball.set_owner(Some(mob));
        fireball.set_rot(b.y_rot(), b.x_rot());
        fireball.explosion_power = explosion_power;
        fireball
    }

    /// `setItem(source)`.
    pub fn set_item(&mut self, source: &ItemStack) {
        // a new stack object: vanilla's identity comparison always sees a change
        self.base
            .entity_data
            .set(DATA_ITEM_STACK, fireball::item_to_store(source));
    }

    /// `getItem()`.
    pub fn get_item(&self) -> ItemStack {
        self.base.entity_data.get(DATA_ITEM_STACK)
    }
}

impl Projectile for LargeFireball {
    hurting_projectile_impl!();

    fn on_hit(&mut self, level: &mut ServerLevel, hit: &HitResult) {
        crate::world::entity::projectile::on_hit_base(self, level, hit);
        let grief = level.mob_griefing();
        let p = self.position();
        let power = self.explosion_power as f32;
        level.explode(
            Explode::new(
                Some(self.as_dyn_mut()),
                p.x,
                p.y,
                p.z,
                power,
                ExplosionInteraction::Mob,
            )
            .with_fire(grief),
        );
        self.discard();
    }

    fn on_hit_entity(&mut self, level: &mut ServerLevel, hit: &EntityHitResult) {
        let victim = hit.entity;
        let owner_id = self.get_owner(level).map(|o| o.id());
        let source = level
            .damage_sources()
            .fireball(self.as_dyn(), owner_id.and_then(|id| level.get_entity(id)));
        level.with_entity(victim, |level, target| {
            level.lend(self.as_dyn_mut(), |level| {
                target.hurt_server(level, &source, 6.0)
            });
            crate::world::item::enchantment_helper::do_post_attack_effects(
                level,
                &mut Actors::of(&mut *target),
                victim,
                &source,
            );
        });
    }
}

impl AbstractHurtingProjectile for LargeFireball {
    hurting_data_impl!();
}

impl Entity for LargeFireball {
    hurting_entity_impl!();

    fn tick(&mut self, level: &mut ServerLevel) {
        super::tick(self, level);
    }

    fn play_entity_on_fire_extinguished_sound(&mut self, _level: &mut ServerLevel) {}

    fn get_slot(&self, slot: i32) -> Option<ItemStack> {
        if slot == 0 {
            return Some(self.get_item());
        }
        crate::advancements::predicates::entity::support::get_slot(self, slot)
    }

    fn set_slot(&mut self, level: &mut ServerLevel, slot: i32, stack: ItemStack) {
        if slot == 0 {
            self.set_item(&stack);
        } else {
            crate::advancements::predicates::entity::support::set_slot(level, self, slot, stack);
        }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        super::add_additional_save_data(self, out);
        fireball::put_item(out, &self.get_item());
        out.put_byte("ExplosionPower", self.explosion_power as i8);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        super::read_additional_save_data(self, input);
        self.set_item(&fireball::read_item(input));
        self.explosion_power =
            i32::from(input.get_byte_or("ExplosionPower", DEFAULT_EXPLOSION_POWER as i8));
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static LARGE_FIREBALL_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::FIREBALL,
    create: |ty, id, _registries| Box::new(LargeFireball::new(ty, id)),
};
