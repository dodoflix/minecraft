//! `WanderingTraderData` (`data/minecraft/wandering_trader.dat`).

use std::sync::LazyLock;

use mc_core::ident::Identifier;
use mc_dfu::INT;
use mc_nbt::Tag;

use super::{SavedData, SavedDataType, decode_codec};
use crate::impl_saved_data;
use crate::util::codec::DecodeResult;

#[derive(Debug, Clone)]
pub struct WanderingTraderData {
    dirty: bool,
    spawn_delay: i32,
    spawn_chance: i32,
}

impl_saved_data!(WanderingTraderData);

impl Default for WanderingTraderData {
    fn default() -> Self {
        WanderingTraderData::new(24000, 25)
    }
}

impl WanderingTraderData {
    pub fn new(spawn_delay: i32, spawn_chance: i32) -> Self {
        WanderingTraderData {
            dirty: false,
            spawn_delay,
            spawn_chance,
        }
    }

    pub fn spawn_delay(&self) -> i32 {
        self.spawn_delay
    }
    pub fn set_spawn_delay(&mut self, v: i32) {
        if self.spawn_delay != v {
            self.spawn_delay = v;
            self.set_dirty_to(true);
        }
    }
    pub fn spawn_chance(&self) -> i32 {
        self.spawn_chance
    }
    pub fn set_spawn_chance(&mut self, v: i32) {
        if self.spawn_chance != v {
            self.spawn_chance = v;
            self.set_dirty_to(true);
        }
    }

    /// `WanderingTraderData.CODEC`.
    pub fn codec() -> mc_dfu::Codec<WanderingTraderData> {
        static CODEC: LazyLock<mc_dfu::Codec<WanderingTraderData>> = LazyLock::new(|| {
            mc_dfu::record!(Packed {
                spawn_delay: INT.optional_field_of_default("spawn_delay", 24000),
                spawn_chance: INT.optional_field_of_default("spawn_chance", 25),
            })
            .codec()
            .xmap(
                |p| WanderingTraderData::new(p.spawn_delay, p.spawn_chance),
                |d: &WanderingTraderData| Packed {
                    spawn_delay: d.spawn_delay,
                    spawn_chance: d.spawn_chance,
                },
            )
        });
        CODEC.clone()
    }

    pub fn decode(tag: &Tag) -> DecodeResult<WanderingTraderData> {
        decode_codec(&WanderingTraderData::codec(), tag)
    }

    pub fn type_() -> SavedDataType<WanderingTraderData> {
        SavedDataType::from_codec(
            Identifier::parse("minecraft:wandering_trader").expect("static id"),
            WanderingTraderData::default,
            WanderingTraderData::codec(),
            mc_datafix::DataFixTypes::SAVED_DATA_WANDERING_TRADER,
        )
    }
}

/// The serialized shape of [`WanderingTraderData`].
struct Packed {
    spawn_delay: i32,
    spawn_chance: i32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::level::saveddata::SavedDataManager;
    use mc_world::level_storage::SavedDataStorage;

    /// The spawner's shared copy goes into the server store when dirty (`sync_shared_data_to_saved_data`), is written by the
    /// save and is read back into the shared copy at the next load (`load_scoreboard`).
    #[test]
    fn timers_survive_a_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let ty = WanderingTraderData::type_();
        let mut store = SavedDataManager::new(SavedDataStorage::new(dir.path().join("data")));
        // the load reads a file that exists and leaves the entry absent otherwise: an unchanged timer writes no file
        assert!(store.get(&ty).is_none());
        store.save_and_join();
        assert!(
            !dir.path()
                .join("data/minecraft/wandering_trader.dat")
                .exists()
        );
        let mut shared = WanderingTraderData::default();
        assert_eq!((shared.spawn_delay(), shared.spawn_chance()), (24000, 25));
        shared.set_spawn_delay(1234);
        shared.set_spawn_chance(40);
        assert!(shared.is_dirty());
        *store.compute_if_absent(&ty) = shared.clone();
        store.save_and_join();
        assert!(
            dir.path()
                .join("data/minecraft/wandering_trader.dat")
                .is_file()
        );

        let mut reloaded = SavedDataManager::new(SavedDataStorage::new(dir.path().join("data")));
        let loaded = reloaded.get(&ty).expect("saved timers").clone();
        assert_eq!((loaded.spawn_delay(), loaded.spawn_chance()), (1234, 40));
        assert!(!loaded.is_dirty());
    }
}
