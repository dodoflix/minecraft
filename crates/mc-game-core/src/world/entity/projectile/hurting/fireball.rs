//! `net.minecraft.world.entity.projectile.hurtingprojectile.Fireball` (the abstract base of `LargeFireball` and
//! `SmallFireball`: the synched item shown by the client) plus the macros that write the structural trait impls every
//! hurting projectile shares.

use crate::define_accessor;
use crate::world::item::{ItemStack, ItemStackExt, load_item_stack};
use mc_data::Item;
use mc_nbt::{CompoundTag, Tag};

define_accessor!(pub DATA_ITEM_STACK: ItemStack = Fireball);

/// `Fireball.getDefaultItem()`.
pub fn get_default_item() -> ItemStack {
    ItemStack::new(Item::FIRE_CHARGE)
}

/// `Fireball.setItem(source)` for the value to store.
pub fn item_to_store(source: &ItemStack) -> ItemStack {
    if source.is_empty() { get_default_item() } else { source.copy_with_count(1) }
}

/// `Fireball.addAdditionalSaveData`'s `Item` tag.
pub fn put_item(out: &mut CompoundTag, item: &ItemStack) {
    let mut tag = CompoundTag::new();
    item.save_into(&mut tag);
    out.put("Item", Tag::Compound(tag));
}

/// `Fireball.readAdditionalSaveData`'s `Item` (`orElse(getDefaultItem())`).
pub fn read_item(input: &CompoundTag) -> ItemStack {
    input.get_compound("Item").and_then(load_item_stack).unwrap_or_else(get_default_item)
}

/// `Projectile` impl lines shared by every hurting projectile (`AbstractHurtingProjectile.canHitEntity` and
/// `onDeflection`); `$self.projectile` is the `ProjectileData` field.
macro_rules! hurting_projectile_impl {
    () => {
        fn projectile_data(&self) -> &$crate::world::entity::projectile::ProjectileData {
            &self.projectile
        }
        fn projectile_data_mut(&mut self) -> &mut $crate::world::entity::projectile::ProjectileData {
            &mut self.projectile
        }
        fn can_hit_entity(&self, level: &$crate::server::level::ServerLevel, entity: &dyn $crate::world::entity::Entity) -> bool {
            $crate::world::entity::projectile::hurting::can_hit_entity(self, level, entity)
        }
        fn on_deflection(&mut self, _level: &mut $crate::server::level::ServerLevel, by_attack: bool) {
            $crate::world::entity::projectile::hurting::on_deflection(self, by_attack);
        }
    };
}

/// `AbstractHurtingProjectile` data accessors (`hurting` field).
macro_rules! hurting_data_impl {
    () => {
        fn hurting_data(&self) -> &$crate::world::entity::projectile::hurting::HurtingProjectileData {
            &self.hurting
        }
        fn hurting_data_mut(&mut self) -> &mut $crate::world::entity::projectile::hurting::HurtingProjectileData {
            &mut self.hurting
        }
        fn as_hurting_mut(&mut self) -> &mut dyn $crate::world::entity::projectile::hurting::AbstractHurtingProjectile {
            self
        }
    };
}

/// The structural `Entity` lines of a hurting projectile (`base` field), `impl_projectile_entity!(hurting)` and the
/// `hurtServer` of `AbstractHurtingProjectile` (`return false` after `markHurt` is not part of it: vanilla's
/// `AbstractHurtingProjectile.hurtServer` is simply `false`).
macro_rules! hurting_entity_impl {
    () => {
        fn base(&self) -> &$crate::world::entity::EntityData {
            &self.base
        }
        fn base_mut(&mut self) -> &mut $crate::world::entity::EntityData {
            &mut self.base
        }
        fn as_any(&self) -> &dyn ::std::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn ::std::any::Any {
            self
        }
        fn as_dyn(&self) -> &dyn $crate::world::entity::Entity {
            self
        }
        fn as_dyn_mut(&mut self) -> &mut dyn $crate::world::entity::Entity {
            self
        }
        $crate::impl_projectile_entity!(hurting);
        fn hurt_server(&mut self, _level: &mut $crate::server::level::ServerLevel, _source: &$crate::world::damagesource::DamageSource, _amount: f32) -> bool {
            false
        }
    };
}

pub(super) use {hurting_data_impl, hurting_entity_impl, hurting_projectile_impl};
