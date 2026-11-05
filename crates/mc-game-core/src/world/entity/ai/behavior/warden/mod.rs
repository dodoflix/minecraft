//! `net.minecraft.world.entity.ai.behavior.warden`: the warden's behaviors.

pub mod digging;
pub mod emerging;
pub mod force_unmount;
pub mod roar;
pub mod set_roar_target;
pub mod set_warden_look_target;
pub mod sniffing;
pub mod sonic_boom;
pub mod try_to_sniff;

use crate::world::entity::Entity;
use crate::world::entity::monster::warden::Warden;

/// The warden behind a behavior's `body` (every behavior of this package is registered by `WardenAi` only).
pub(crate) fn warden(body: &dyn Entity) -> &Warden {
    body.as_any().downcast_ref::<Warden>().expect("warden behavior on a non-warden body")
}

/// Mutable [`warden`].
pub(crate) fn warden_mut(body: &mut dyn Entity) -> &mut Warden {
    body.as_any_mut().downcast_mut::<Warden>().expect("warden behavior on a non-warden body")
}
