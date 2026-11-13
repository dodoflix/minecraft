//! `net.minecraft.world.entity.projectile.hurtingprojectile.SmallFireball` (blaze fireball, dispenser fire charge).

use super::fireball::{
    self, DATA_ITEM_STACK, hurting_data_impl, hurting_entity_impl, hurting_projectile_impl,
};
use super::{AbstractHurtingProjectile, HurtingProjectileData};
use crate::server::level::ServerLevel;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::projectile::{EntityHitResult, HitResult, Projectile, ProjectileData};
use crate::world::entity::{Entity, EntityData, EntityId, entity};
use crate::world::item::ItemStack;
use crate::world::item::enchantment::actors::Actors;
use crate::world::level::{LevelAccessor, LevelReader};
use linkme::distributed_slice;
use mc_core::phys::{BlockHitResult, Vec3};
use mc_data::EntityType;
use mc_nbt::CompoundTag;

/// `SmallFireball`.
pub struct SmallFireball {
    base: EntityData,
    projectile: ProjectileData,
    hurting: HurtingProjectileData,
}

impl SmallFireball {
    /// `SmallFireball(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let base = EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_ITEM_STACK, fireball::get_default_item());
        });
        Self {
            base,
            projectile: ProjectileData::default(),
            hurting: HurtingProjectileData::default(),
        }
    }

    /// `SmallFireball(level, mob, direction)`.
    pub fn from_mob(level: &mut ServerLevel, mob: &dyn Entity, direction: Vec3) -> Self {
        let mut fireball = Self::new(EntityType::SMALL_FIREBALL, level.next_entity_id());
        let b = mob.base();
        super::init_with_direction(&mut fireball, b.x(), b.y(), b.z(), direction);
        fireball.set_owner(Some(mob));
        fireball.set_rot(b.y_rot(), b.x_rot());
        fireball
    }

    /// `SmallFireball(level, x, y, z, direction)`.
    pub fn at(level: &mut ServerLevel, x: f64, y: f64, z: f64, direction: Vec3) -> Self {
        let mut fireball = Self::new(EntityType::SMALL_FIREBALL, level.next_entity_id());
        super::init_with_direction(&mut fireball, x, y, z, direction);
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

impl Projectile for SmallFireball {
    hurting_projectile_impl!();

    fn on_hit(&mut self, level: &mut ServerLevel, hit: &HitResult) {
        crate::world::entity::projectile::on_hit_base(self, level, hit);
        self.discard_in_level(level);
    }

    fn on_hit_entity(&mut self, level: &mut ServerLevel, hit: &EntityHitResult) {
        let victim = hit.entity;
        let owner_id = self.get_owner(level).map(|o| o.id());
        let source = level
            .damage_sources()
            .fireball(self.as_dyn(), owner_id.and_then(|id| level.get_entity(id)));
        level.with_entity(victim, |level, target| {
            let remaining_fire_ticks = target.get_remaining_fire_ticks();
            target.ignite_for_seconds(5.0);
            if !level.lend(self.as_dyn_mut(), |level| {
                target.hurt_server(level, &source, 5.0)
            }) {
                target.set_remaining_fire_ticks(remaining_fire_ticks);
            } else {
                crate::world::item::enchantment_helper::do_post_attack_effects(
                    level,
                    &mut Actors::of(&mut *target),
                    victim,
                    &source,
                );
            }
        });
    }

    fn on_hit_block(&mut self, level: &mut ServerLevel, hit: &BlockHitResult) {
        crate::world::entity::projectile::on_hit_block_base(self, level, hit);
        let owner_is_mob = self.get_owner(level).is_some_and(|o| o.is_mob());
        if !owner_is_mob || level.mob_griefing() {
            let pos = hit.block_pos.relative(hit.direction);
            if level.is_empty_block(pos) {
                let state =
                    crate::world::level::block::impls::base_fire_block::get_state(&*level, pos);
                level.set_block_and_update(pos, state);
            }
        }
    }
}

impl AbstractHurtingProjectile for SmallFireball {
    hurting_data_impl!();
}

impl Entity for SmallFireball {
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
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        super::read_additional_save_data(self, input);
        self.set_item(&fireball::read_item(input));
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static SMALL_FIREBALL_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::SMALL_FIREBALL,
    create: |ty, id, _registries| Box::new(SmallFireball::new(ty, id)),
};
