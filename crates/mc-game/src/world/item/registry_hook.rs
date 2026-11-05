//! The registries behavior of item classes reads without a level at hand (`Item.getUseDuration` of a goat horn needs its
//! `Instrument`): installed once when the server's levels are created (a process runs one server).

use crate::registry::RegistryAccess;
use std::sync::RwLock;

static REGISTRIES: RwLock<Option<RegistryAccess>> = RwLock::new(None);

/// Installs the server's composite registries (`ServerLevel.registries`).
pub fn set_item_registries(registries: RegistryAccess) {
    *REGISTRIES.write().unwrap_or_else(|e| e.into_inner()) = Some(registries);
}

/// Runs `f` with the installed registries (`None` before a server created its levels).
pub fn with_item_registries<R>(f: impl FnOnce(&RegistryAccess) -> R) -> Option<R> {
    REGISTRIES.read().unwrap_or_else(|e| e.into_inner()).as_ref().map(f)
}
