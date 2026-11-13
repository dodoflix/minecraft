//! `net.minecraft.world.entity.vehicle.minecart`: `AbstractMinecart` (the state and behaviour every cart shares), the default
//! `OldMinecartBehavior` and the rideable `Minecart`.
//!
//! A concrete cart embeds an `EntityData` and a [`abstract_minecart::MinecartData`], implements [`AbstractMinecart`] (the overridable
//! vanilla methods; defaults are `AbstractMinecart`'s) and [`Entity`](crate::world::entity::Entity) through
//! [`abstract_minecart::minecart_entity_methods`] plus its own `tick` / `interact` / `get_pick_result` / save methods.
//!
//! Not implemented: `NewMinecartBehavior` (the `minecart_improvements` experiment): every cart moves with the old behaviour.

pub mod abstract_minecart;
#[expect(
    clippy::module_inception,
    reason = "mirrors the vanilla package and class name"
)]
pub mod minecart;
pub mod old_behavior;

pub use abstract_minecart::{AbstractMinecart, MinecartData};
