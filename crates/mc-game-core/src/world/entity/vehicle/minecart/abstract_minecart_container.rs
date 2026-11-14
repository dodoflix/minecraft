//! `net.minecraft.world.entity.vehicle.minecart.AbstractMinecartContainer`: what the chest and hopper minecarts share on top of
//! `AbstractMinecart`. A cart embeds a [`ChestVehicleData`] (`chest`) besides `base` and `minecart`, implements [`ContainerEntity`]
//! and [`AbstractMinecart`], and forwards its `Entity` overrides here (the container accessors come from
//! [`minecart_container_entity_methods`]).

use super::abstract_minecart::{self, AbstractMinecart};
use crate::server::level::ServerLevel;
use crate::world::container::get_redstone_signal_from_container;
use crate::world::damagesource::DamageSource;
use crate::world::entity::RemovalReason;
use crate::world::entity::scenario_dump::item_full;
use crate::world::entity::vehicle::container_entity::{ChestVehicleData, ContainerEntity};
use crate::world::entity::vehicle::vehicle_entity;
use mc_core::phys::Vec3;
use mc_nbt::CompoundTag;

/// `destroy(level, source)`: the item, then `chestVehicleDestroyed`.
pub fn destroy<M: AbstractMinecart + ContainerEntity + ?Sized>(
    m: &mut M,
    level: &mut ServerLevel,
    source: &DamageSource,
) {
    let item = m.get_drop_item();
    vehicle_entity::destroy_item(m, level, item);
    m.chest_vehicle_destroyed(source, level);
}

/// `remove(reason)`: the contents drop before the entity is removed.
pub fn before_remove<M: ContainerEntity + ?Sized>(
    m: &mut M,
    level: &mut ServerLevel,
    reason: RemovalReason,
) {
    if reason.should_destroy() {
        m.drop_chest_contents(level);
    }
}

/// `applyNaturalSlowdown(deltaMovement)`: the fuller the cart the more it slows; a cart with a pending loot table keeps 0.98.
pub fn apply_natural_slowdown<M: AbstractMinecart + ContainerEntity + ?Sized>(
    m: &M,
    movement: Vec3,
) -> Vec3 {
    let mut keep = 0.98f32;
    if !m.chest().has_loot_table() {
        let emptiness = 15 - get_redstone_signal_from_container(Some(m.chest()));
        keep += emptiness as f32 * 0.001f32;
    }
    if m.is_in_water() {
        keep *= 0.95f32;
    }
    movement.multiply_xyz(f64::from(keep), 0.0, f64::from(keep))
}

/// `addAdditionalSaveData(output)`.
pub fn add_additional_save_data<M: AbstractMinecart + ContainerEntity + ?Sized>(
    m: &M,
    out: &mut CompoundTag,
) {
    abstract_minecart::add_additional_save_data(m, out);
    m.chest().add_chest_vehicle_save_data(out);
}

/// `readAdditionalSaveData(input)`.
pub fn read_additional_save_data<M: AbstractMinecart + ContainerEntity + ?Sized>(
    m: &mut M,
    level: Option<&ServerLevel>,
    input: &CompoundTag,
) {
    abstract_minecart::read_additional_save_data(m, level, input);
    m.chest_mut().read_chest_vehicle_save_data(input);
}

/// The scenario dump fields of a container cart: `items loot seed` (the raw item list, a pending loot table stays pending).
pub fn dump_fields(chest: &ChestVehicleData) -> Vec<(String, String)> {
    let items: Vec<String> = chest
        .get_item_stacks()
        .iter()
        .enumerate()
        .filter(|(_, stack)| !stack.is_empty())
        .map(|(slot, stack)| format!("{slot}:{}", item_full(stack)))
        .collect();
    vec![
        (
            "items".into(),
            if items.is_empty() {
                "-".into()
            } else {
                items.join(";")
            },
        ),
        (
            "loot".into(),
            chest
                .get_container_loot_table()
                .map_or_else(|| "-".into(), ToString::to_string),
        ),
        (
            "seed".into(),
            chest.get_container_loot_table_seed().to_string(),
        ),
    ]
}

/// The `Entity` methods that expose the [`ChestVehicleData`] of a cart (the struct needs the field `chest`); expands inside
/// `impl Entity for X`.
#[macro_export]
macro_rules! minecart_container_entity_methods {
    () => {
        fn as_container(&self) -> Option<&dyn $crate::world::container::Container> {
            Some(&self.chest)
        }
        fn as_container_mut(&mut self) -> Option<&mut dyn $crate::world::container::Container> {
            Some(&mut self.chest)
        }
        fn chest_vehicle(
            &self,
        ) -> Option<&$crate::world::entity::vehicle::container_entity::ChestVehicleData> {
            Some(&self.chest)
        }
        fn chest_vehicle_mut(
            &mut self,
        ) -> Option<&mut $crate::world::entity::vehicle::container_entity::ChestVehicleData> {
            Some(&mut self.chest)
        }
        fn as_menu_provider(
            &self,
            level: &$crate::server::level::ServerLevel,
        ) -> Option<Box<dyn $crate::world::inventory::MenuProvider>> {
            Some(Box::new(
                $crate::world::entity::vehicle::container_entity::ChestVehicleMenuProvider::new(
                    $crate::world::entity::Entity::id(self),
                    $crate::world::damagesource::display_name::entity_display_name(level, self),
                ),
            ))
        }
        fn before_remove(
            &mut self,
            level: &mut $crate::server::level::ServerLevel,
            reason: $crate::world::entity::RemovalReason,
        ) {
            $crate::world::entity::vehicle::minecart::abstract_minecart_container::before_remove(
                self, level, reason,
            );
        }
    };
}
