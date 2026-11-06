//! `net.minecraft.world.entity.animal.fish`: `WaterAnimal`, `AbstractFish`, `AbstractSchoolingFish` and the four fish.

pub mod abstract_fish;
pub mod abstract_schooling_fish;
pub mod cod;
pub mod pufferfish;
pub mod salmon;
pub mod tropical_fish;
pub mod water_animal;

pub use abstract_fish::AbstractFish;
pub use abstract_schooling_fish::SchoolingFish;
pub use cod::Cod;
pub use pufferfish::Pufferfish;
pub use salmon::Salmon;
pub use tropical_fish::TropicalFish;

use mc_game_core::world::entity::Entity;

/// `entity instanceof AbstractFish` (the concrete fish with `FishSwimGoal`).
pub fn as_abstract_fish(e: &dyn Entity) -> Option<&dyn AbstractFish> {
    let any = e.as_any();
    if let Some(f) = any.downcast_ref::<Cod>() {
        Some(f)
    } else if let Some(f) = any.downcast_ref::<Salmon>() {
        Some(f)
    } else if let Some(f) = any.downcast_ref::<TropicalFish>() {
        Some(f)
    } else if let Some(f) = any.downcast_ref::<Pufferfish>() {
        Some(f)
    } else {
        None
    }
}

/// `entity instanceof AbstractSchoolingFish`.
pub fn as_schooling_fish(e: &dyn Entity) -> Option<&dyn SchoolingFish> {
    let any = e.as_any();
    if let Some(f) = any.downcast_ref::<Cod>() {
        Some(f)
    } else if let Some(f) = any.downcast_ref::<Salmon>() {
        Some(f)
    } else if let Some(f) = any.downcast_ref::<TropicalFish>() {
        Some(f)
    } else {
        None
    }
}

/// Mutable form of [`as_schooling_fish`].
pub fn as_schooling_fish_mut(e: &mut dyn Entity) -> Option<&mut dyn SchoolingFish> {
    let is_cod = e.as_any().is::<Cod>();
    let is_salmon = e.as_any().is::<Salmon>();
    let any = e.as_any_mut();
    if is_cod {
        any.downcast_mut::<Cod>()
            .map(|f| f as &mut dyn SchoolingFish)
    } else if is_salmon {
        any.downcast_mut::<Salmon>()
            .map(|f| f as &mut dyn SchoolingFish)
    } else {
        any.downcast_mut::<TropicalFish>()
            .map(|f| f as &mut dyn SchoolingFish)
    }
}
