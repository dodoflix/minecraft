//! `net.minecraft.world.entity.projectile.throwableitemprojectile.ThrownEgg`.

use super::super::throwable::{self, DATA_ITEM_STACK};
use super::super::{EntityHitResult, HitResult, Projectile, ProjectileData, on_hit_base};
use crate::server::level::ServerLevel;
use crate::world::entity::animal::chicken::chicken::Chicken;
use crate::world::entity::dimensions::EntityDimensions;
use crate::world::entity::entity::TOTAL_AIR_SUPPLY;
use crate::world::entity::factory::{
    self, ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use crate::world::entity::mob::ageable;
use crate::world::entity::movement::fudge_position_after_size_change;
use crate::world::entity::{Entity, EntityData, EntityId};
use crate::world::item::ItemStack;
use crate::{impl_throwable_entity, impl_throwable_projectile};
use linkme::distributed_slice;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item};
use mc_nbt::CompoundTag;

pub struct ThrownEgg {
    base: EntityData,
    projectile: ProjectileData,
}

impl ThrownEgg {
    /// `ThrownEgg(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let base = EntityData::new(ty, id, TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_ITEM_STACK, ItemStack::new(Item::EGG));
        });
        Self {
            base,
            projectile: ProjectileData::default(),
        }
    }

    /// `ThrownEgg(level, mob, itemStack)`.
    pub fn with_owner(level: &ServerLevel, owner: &dyn Entity, item: &ItemStack) -> Self {
        let mut s = Self::new(EntityType::EGG, level.next_entity_id());
        throwable::position_at_owner(&mut s, owner);
        throwable::set_item(&mut s.base, item);
        s.set_owner(Some(owner));
        s
    }

    /// `ThrownEgg(level, x, y, z, itemStack)`.
    pub fn at(level: &ServerLevel, x: f64, y: f64, z: f64, item: &ItemStack) -> Self {
        let mut s = Self::new(EntityType::EGG, level.next_entity_id());
        s.set_pos(x, y, z);
        throwable::set_item(&mut s.base, item);
        s
    }
}

impl Projectile for ThrownEgg {
    impl_throwable_projectile!();

    fn on_hit_entity(&mut self, level: &mut ServerLevel, hit: &EntityHitResult) {
        let source = level
            .damage_sources()
            .thrown(self.as_dyn(), self.get_owner(level));
        level.with_entity(hit.entity, |level, e| e.hurt_server(level, &source, 0.0));
    }

    fn on_hit(&mut self, level: &mut ServerLevel, hit: &HitResult) {
        on_hit_base(self, level, hit);
        if self.base.random.next_int_bounded(8) == 0 {
            let mut count = 1;
            if self.base.random.next_int_bounded(32) == 0 {
                count = 4;
            }
            let item = throwable::get_item(&self.base);
            for _ in 0..count {
                let request = EntitySpawnRequest {
                    reason: EntitySpawnReason::Triggered,
                    ignore_checks: false,
                };
                let Some(mut chicken) = factory::create(EntityType::CHICKEN, level, request) else {
                    continue;
                };
                ageable::set_age_in_level(chicken.as_mut(), level, -24000);
                chicken.snap_to(
                    self.base.x(),
                    self.base.y(),
                    self.base.z(),
                    self.base.y_rot(),
                    0.0,
                );
                if let Some(chicken) = chicken.as_any_mut().downcast_mut::<Chicken>() {
                    chicken.apply_variant_component(&item);
                }
                if !fudge_position_after_size_change(
                    chicken.as_mut(),
                    level,
                    &EntityDimensions::fixed(0.0, 0.0),
                ) {
                    break;
                }
                level.add_fresh_entity(chicken);
            }
        }
        level.broadcast_entity_event(self.id(), 3);
        self.discard_in_level(level);
    }
}

impl Entity for ThrownEgg {
    impl_throwable_entity!(0.03);

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        throwable::add_additional_save_data(self, out);
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        throwable::read_additional_save_data(self, level, input, Item::EGG);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static EGG_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::EGG,
    create: |ty, id, _| Box::new(ThrownEgg::new(ty, id)),
};
