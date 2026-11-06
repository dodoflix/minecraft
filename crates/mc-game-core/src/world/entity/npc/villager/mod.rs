//! `net.minecraft.world.entity.npc.villager`.

pub mod abstract_villager;
pub mod villager;
pub mod villager_data;
pub mod villager_profession;
pub mod villager_type;

pub use villager::Villager;
pub use villager_data::{VillagerData, VillagerDataExt};

use crate::world::entity::Entity;

/// `entity instanceof Villager`.
pub fn villager_ref(e: &dyn Entity) -> Option<&Villager> {
    e.as_any().downcast_ref::<Villager>()
}

/// A behavior typed `Behavior<Villager>` only ever runs on villagers.
pub fn villager_mut(e: &mut dyn Entity) -> &mut Villager {
    e.as_any_mut()
        .downcast_mut::<Villager>()
        .expect("villager behavior on a non-villager")
}
