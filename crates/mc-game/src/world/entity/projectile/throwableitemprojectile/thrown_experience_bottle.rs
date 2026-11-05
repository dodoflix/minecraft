//! `net.minecraft.world.entity.projectile.throwableitemprojectile.ThrownExperienceBottle`.

use super::super::throwable::{self, DATA_ITEM_STACK};
use super::super::{HitResult, Projectile, ProjectileData, on_hit_base};
use crate::server::level::ServerLevel;
use crate::world::entity::entity::TOTAL_AIR_SUPPLY;
use crate::world::entity::experience_orb::ExperienceOrb;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::{Entity, EntityData, EntityId};
use crate::world::item::ItemStack;
use crate::world::level::LevelAccessor;
use crate::{impl_throwable_entity, impl_throwable_projectile};
use linkme::distributed_slice;
use mc_core::random::RandomSource;
use mc_data::{EntityType, Item};
use mc_nbt::CompoundTag;

pub struct ThrownExperienceBottle {
    base: EntityData,
    projectile: ProjectileData,
}

impl ThrownExperienceBottle {
    /// `ThrownExperienceBottle(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let base = EntityData::new(ty, id, TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_ITEM_STACK, ItemStack::new(Item::EXPERIENCE_BOTTLE));
        });
        Self { base, projectile: ProjectileData::default() }
    }

    /// `ThrownExperienceBottle(level, mob, itemStack)`.
    pub fn with_owner(level: &ServerLevel, owner: &dyn Entity, item: &ItemStack) -> Self {
        let mut s = Self::new(EntityType::EXPERIENCE_BOTTLE, level.next_entity_id());
        throwable::position_at_owner(&mut s, owner);
        throwable::set_item(&mut s.base, item);
        s.set_owner(Some(owner));
        s
    }

    /// `ThrownExperienceBottle(level, x, y, z, itemStack)`.
    pub fn at(level: &ServerLevel, x: f64, y: f64, z: f64, item: &ItemStack) -> Self {
        let mut s = Self::new(EntityType::EXPERIENCE_BOTTLE, level.next_entity_id());
        s.set_pos(x, y, z);
        throwable::set_item(&mut s.base, item);
        s
    }
}

impl Projectile for ThrownExperienceBottle {
    impl_throwable_projectile!();

    fn on_hit(&mut self, level: &mut ServerLevel, hit: &HitResult) {
        on_hit_base(self, level, hit);
        let pos = self.block_position();
        level.level_event(None, 2002, pos, -13083194);
        if !self.base.is_silent() {
            level.level_event(None, 1053, pos, 0);
        }
        let xp_count = 3 + self.base.random.next_int_bounded(5) + self.base.random.next_int_bounded(5);
        match hit {
            HitResult::Block(block_hit) => ExperienceOrb::award_with_direction(level, &hit.get_location(), block_hit.direction.get_unit_vec3(), xp_count),
            HitResult::Entity(_) => ExperienceOrb::award_with_direction(level, &hit.get_location(), self.base.delta_movement().scale(-1.0), xp_count),
        }
        self.discard();
    }
}

impl Entity for ThrownExperienceBottle {
    impl_throwable_entity!(0.07);

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        throwable::add_additional_save_data(self, out);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        throwable::read_additional_save_data(self, input, Item::EXPERIENCE_BOTTLE);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static EXPERIENCE_BOTTLE_FACTORY: EntityRegistration =
    EntityRegistration { entity_type: EntityType::EXPERIENCE_BOTTLE, create: |ty, level| Box::new(ThrownExperienceBottle::new(ty, level.next_entity_id())) };
