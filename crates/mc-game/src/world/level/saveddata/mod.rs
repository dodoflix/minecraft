//! `net.minecraft.world.level.saveddata` + `world.level.storage.SavedDataStorage`: the typed `SavedData`
//! framework over mc-world's raw [`mc_world::level_storage::SavedDataStorage`], and the saved-data instances.
mod manager;
pub mod command_storage;
pub mod raids;
pub mod maps;
pub mod random_sequences;
pub mod stopwatches;
pub mod wandering_trader_data;
pub mod weather_data;
pub mod world_border;

pub use manager::{PendingSave, SavedData, SavedDataManager, SavedDataType};
