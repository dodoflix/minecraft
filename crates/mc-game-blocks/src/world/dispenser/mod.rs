//! The dispenser behaviors of `DispenseItemBehavior.bootStrap()` that act on blocks and block entities, registered into
//! the core dispenser registry.
//!
//! Not implemented: shears on `CopperGolem` (it has no `Shearable` hook yet), and the `WitherBoss` summoned by a
//! dispensed wither skull (`check_spawn` of the wither skull block only creates an inert entity).

mod carved_pumpkin;
mod glass_bottle;
mod glowstone;
mod shears;
mod wither_skull;
