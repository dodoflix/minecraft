//! `net.minecraft.world.entity.projectile.throwableitemprojectile.Snowball`.

use super::super::throwable::{self, DATA_ITEM_STACK};
use super::super::{EntityHitResult, HitResult, Projectile, ProjectileData, on_hit_base};
use crate::server::level::ServerLevel;
use crate::world::entity::entity::TOTAL_AIR_SUPPLY;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::{Entity, EntityData, EntityId};
use crate::world::item::ItemStack;
use crate::{impl_throwable_entity, impl_throwable_projectile};
use linkme::distributed_slice;
use mc_data::{EntityType, Item};
use mc_nbt::CompoundTag;

pub struct Snowball {
    base: EntityData,
    projectile: ProjectileData,
}

impl Snowball {
    /// `Snowball(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let base = EntityData::new(ty, id, TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_ITEM_STACK, ItemStack::new(Item::SNOWBALL));
        });
        Self {
            base,
            projectile: ProjectileData::default(),
        }
    }

    /// `Snowball(level, mob, itemStack)`.
    pub fn with_owner(level: &ServerLevel, owner: &dyn Entity, item: &ItemStack) -> Self {
        let mut s = Self::new(EntityType::SNOWBALL, level.next_entity_id());
        throwable::position_at_owner(&mut s, owner);
        throwable::set_item(&mut s.base, item);
        s.set_owner(Some(owner));
        s
    }

    /// `Snowball(level, x, y, z, itemStack)`.
    pub fn at(level: &ServerLevel, x: f64, y: f64, z: f64, item: &ItemStack) -> Self {
        let mut s = Self::new(EntityType::SNOWBALL, level.next_entity_id());
        s.set_pos(x, y, z);
        throwable::set_item(&mut s.base, item);
        s
    }
}

impl Projectile for Snowball {
    impl_throwable_projectile!();

    fn on_hit_entity(&mut self, level: &mut ServerLevel, hit: &EntityHitResult) {
        let damage = if level
            .get_entity(hit.entity)
            .is_some_and(|e| e.get_type() == EntityType::BLAZE)
        {
            3.0
        } else {
            0.0
        };
        let source = level
            .damage_sources()
            .thrown(self.as_dyn(), self.get_owner(level));
        level.with_entity(hit.entity, |level, e| {
            level.lend(self.as_dyn_mut(), |level| {
                e.hurt_server(level, &source, damage)
            })
        });
    }

    fn on_hit(&mut self, level: &mut ServerLevel, hit: &HitResult) {
        on_hit_base(self, level, hit);
        level.broadcast_entity_event(self.id(), 3);
        self.discard_in_level(level);
    }
}

impl Entity for Snowball {
    impl_throwable_entity!(0.03);

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        throwable::add_additional_save_data(self, out);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        throwable::read_additional_save_data(self, input, Item::SNOWBALL);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static SNOWBALL_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::SNOWBALL,
    create: |ty, id, _| Box::new(Snowball::new(ty, id)),
};
