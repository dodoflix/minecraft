//! `net.minecraft.world.entity.projectile.throwableitemprojectile.ThrownLingeringPotion`.

use super::super::throwable::{self, DATA_ITEM_STACK};
use super::super::{HitResult, Projectile, ProjectileData};
use super::abstract_thrown_potion;
use crate::server::level::ServerLevel;
use crate::world::entity::area_effect_cloud::AreaEffectCloud;
use crate::world::entity::entity::TOTAL_AIR_SUPPLY;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::{Entity, EntityData, EntityId};
use crate::world::item::ItemStack;
use crate::{impl_throwable_entity, impl_throwable_projectile};
use linkme::distributed_slice;
use mc_core::phys::BlockHitResult;
use mc_data::{EntityType, Item};
use mc_nbt::CompoundTag;

pub struct ThrownLingeringPotion {
    base: EntityData,
    projectile: ProjectileData,
}

impl ThrownLingeringPotion {
    /// `ThrownLingeringPotion(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let base = EntityData::new(ty, id, TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_ITEM_STACK, ItemStack::new(Item::LINGERING_POTION));
        });
        Self {
            base,
            projectile: ProjectileData::default(),
        }
    }

    /// `ThrownLingeringPotion(level, owner, itemStack)`.
    pub fn with_owner(level: &ServerLevel, owner: &dyn Entity, item: &ItemStack) -> Self {
        let mut s = Self::new(EntityType::LINGERING_POTION, level.next_entity_id());
        throwable::position_at_owner(&mut s, owner);
        throwable::set_item(&mut s.base, item);
        s.set_owner(Some(owner));
        s
    }

    /// `ThrownLingeringPotion(level, x, y, z, itemStack)`.
    pub fn at(level: &ServerLevel, x: f64, y: f64, z: f64, item: &ItemStack) -> Self {
        let mut s = Self::new(EntityType::LINGERING_POTION, level.next_entity_id());
        s.set_pos(x, y, z);
        throwable::set_item(&mut s.base, item);
        s
    }
}

/// `onHitAsPotion(level, potionItem, hitResult)`: the cloud appears at the hit entity, else at the potion.
fn on_hit_as_potion(
    p: &mut dyn Projectile,
    level: &mut ServerLevel,
    potion_item: &ItemStack,
    hit: &HitResult,
) {
    let at = match hit {
        HitResult::Entity(h) => level.get_entity(h.entity).map(|e| e.position()),
        HitResult::Block(_) => None,
    }
    .unwrap_or_else(|| p.position());
    let mut cloud = AreaEffectCloud::at(level, at.x, at.y, at.z);
    if let Some(owner) = p.get_owner(level).filter(|o| o.living().is_some()) {
        cloud.set_owner(Some(owner));
    }
    cloud.set_radius(3.0);
    cloud.set_radius_on_use(-0.5);
    cloud.set_duration(600);
    cloud.set_wait_time(10);
    cloud.set_radius_per_tick(-cloud.get_radius() / cloud.get_duration() as f32);
    cloud.apply_implicit_components(potion_item);
    level.add_fresh_entity(Box::new(cloud));
}

impl Projectile for ThrownLingeringPotion {
    impl_throwable_projectile!();

    fn on_hit_block(&mut self, level: &mut ServerLevel, hit: &BlockHitResult) {
        abstract_thrown_potion::on_hit_block(self, level, hit);
    }

    fn on_hit(&mut self, level: &mut ServerLevel, hit: &HitResult) {
        abstract_thrown_potion::on_hit(self, level, hit, on_hit_as_potion);
    }
}

impl Entity for ThrownLingeringPotion {
    impl_throwable_entity!(0.05, away);

    fn thrown_potion_hurts_water_sensitive(&self) -> Option<bool> {
        Some(abstract_thrown_potion::hurts_water_sensitive(self))
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        throwable::add_additional_save_data(self, out);
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        throwable::read_additional_save_data(self, level, input, Item::LINGERING_POTION);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static LINGERING_POTION_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::LINGERING_POTION,
    create: |ty, id, _| Box::new(ThrownLingeringPotion::new(ty, id)),
};
