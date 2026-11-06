//! `net.minecraft.world.entity.projectile.arrow.SpectralArrow`: glowing on hit.

use super::{AbstractArrow, AbstractArrowData};
use crate::server::level::ServerLevel;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::projectile::{Projectile, ProjectileData};
use crate::world::entity::{Entity, EntityData, EntityId, entity};
use crate::world::item::{ItemStack, ItemStackExt as _};
use linkme::distributed_slice;
use mc_data::{EntityType, Item, MobEffect, SoundEvent};
use mc_nbt::CompoundTag;
use std::any::Any;

const DEFAULT_DURATION: i32 = 200;

pub struct SpectralArrow {
    base: EntityData,
    projectile: ProjectileData,
    arrow: AbstractArrowData,
    duration: i32,
}

impl SpectralArrow {
    /// `SpectralArrow(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let base = EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, super::define_synched_data);
        Self {
            base,
            projectile: ProjectileData::default(),
            arrow: AbstractArrowData::new(
                SoundEvent::ENTITY_ARROW_HIT,
                ItemStack::new(Item::SPECTRAL_ARROW),
            ),
            duration: DEFAULT_DURATION,
        }
    }

    /// `SpectralArrow(level, owner, pickupItemStack, firedFromWeapon)`.
    pub fn from_owner(
        level: &mut ServerLevel,
        owner: &dyn Entity,
        pickup_item_stack: ItemStack,
        fired_from_weapon: Option<&ItemStack>,
    ) -> Self {
        let mut arrow = Self::new(EntityType::SPECTRAL_ARROW, level.next_entity_id());
        super::init_from_owner(
            &mut arrow,
            level,
            owner,
            pickup_item_stack,
            fired_from_weapon,
        );
        arrow
    }

    /// `SpectralArrow(level, x, y, z, pickupItemStack, firedFromWeapon)`.
    pub fn at(
        level: &mut ServerLevel,
        x: f64,
        y: f64,
        z: f64,
        pickup_item_stack: ItemStack,
        fired_from_weapon: Option<&ItemStack>,
    ) -> Self {
        let mut arrow = Self::new(EntityType::SPECTRAL_ARROW, level.next_entity_id());
        super::init_at(
            &mut arrow,
            level,
            x,
            y,
            z,
            pickup_item_stack,
            fired_from_weapon,
        );
        arrow
    }
}

impl Projectile for SpectralArrow {
    fn projectile_data(&self) -> &ProjectileData {
        &self.projectile
    }
    fn projectile_data_mut(&mut self) -> &mut ProjectileData {
        &mut self.projectile
    }
    crate::impl_abstract_arrow_projectile!();
}

impl AbstractArrow for SpectralArrow {
    fn arrow_data(&self) -> &AbstractArrowData {
        &self.arrow
    }
    fn arrow_data_mut(&mut self) -> &mut AbstractArrowData {
        &mut self.arrow
    }
    fn as_abstract_arrow(&self) -> &dyn AbstractArrow {
        self
    }
    fn as_abstract_arrow_mut(&mut self) -> &mut dyn AbstractArrow {
        self
    }
    fn get_default_pickup_item(&self) -> ItemStack {
        ItemStack::new(Item::SPECTRAL_ARROW)
    }

    fn do_post_hurt_effects(&mut self, level: &mut ServerLevel, mob: &mut dyn Entity) {
        let effect = MobEffectInstance::new(
            MobEffect::GLOWING,
            self.duration,
            0,
            false,
            true,
            true,
            None,
        );
        let source = self.get_effect_source(level);
        mob.add_effect(level, effect, Some(source));
    }
}

impl Entity for SpectralArrow {
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
    crate::impl_abstract_arrow_entity!();

    fn get_weapon_item(&self) -> Option<ItemStack> {
        self.arrow.fired_from_weapon.clone()
    }

    fn player_touch(
        &mut self,
        level: &mut ServerLevel,
        player: &mut crate::world::entity::ServerPlayer,
    ) {
        super::player_touch(self, level, player);
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        super::tick(self, level);
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        super::add_additional_save_data(self, out);
        out.put_int("Duration", self.duration);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        super::read_additional_save_data(self, input);
        self.duration = input.get_int_or("Duration", DEFAULT_DURATION);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static SPECTRAL_ARROW_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::SPECTRAL_ARROW,
    create: |ty, id, _registries| Box::new(SpectralArrow::new(ty, id)),
};
