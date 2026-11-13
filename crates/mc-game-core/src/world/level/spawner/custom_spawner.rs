//! `net.minecraft.world.level.CustomSpawner` and `ServerLevel.customSpawners`/`tickCustomSpawners`.

use crate::server::level::ServerLevel;
use linkme::distributed_slice;

/// `CustomSpawner.tick(level, spawnEnemies)`: the spawn count (vanilla returns it, `ServerLevel` ignores it).
pub trait CustomSpawner: Send {
    fn tick(&mut self, level: &mut ServerLevel, spawn_enemies: bool) -> i32;
}

/// Orders of the vanilla spawners in `MinecraftServer.createLevels` (`ImmutableList.of(new PhantomSpawner(), new
/// PatrolSpawner(), new CatSpawner(), new VillageSiege(), new WanderingTraderSpawner(..))`).
pub mod order {
    pub const PHANTOM: i32 = 0;
    pub const PATROL: i32 = 1;
    pub const CAT: i32 = 2;
    pub const VILLAGE_SIEGE: i32 = 3;
    pub const WANDERING_TRADER: i32 = 4;
}

/// A spawner of a level: `create` runs once when the level is created and returns `None` if this level has none
/// (vanilla gives only the overworld a list; the other dimensions get `List.of()`).
pub struct CustomSpawnerRegistration {
    pub order: i32,
    pub create: fn(&ServerLevel) -> Option<Box<dyn CustomSpawner>>,
}

#[distributed_slice]
pub static CUSTOM_SPAWNERS: [CustomSpawnerRegistration];

impl ServerLevel {
    /// Builds `customSpawners` from the registrations (level factory, once the level knows its dimension).
    pub fn install_custom_spawners(&mut self) {
        let mut registrations: Vec<&CustomSpawnerRegistration> = CUSTOM_SPAWNERS.iter().collect();
        registrations.sort_by_key(|r| r.order);
        let (orders, spawners): (Vec<i32>, Vec<Box<dyn CustomSpawner>>) = registrations
            .into_iter()
            .filter_map(|r| (r.create)(self).map(|spawner| (r.order, spawner)))
            .unzip();
        self.spawning.custom_spawner_orders = orders;
        self.spawning.custom_spawners = spawners;
    }

    /// One element of `customSpawners` (`order::*`): `spawner.tick(level, spawnEnemies)`; `false` when this level has no such spawner.
    pub fn tick_custom_spawner(&mut self, order: i32, spawn_enemies: bool) -> bool {
        let Some(index) = self
            .spawning
            .custom_spawner_orders
            .iter()
            .position(|&o| o == order)
        else {
            return false;
        };
        let mut spawners = std::mem::take(&mut self.spawning.custom_spawners);
        spawners[index].tick(self, spawn_enemies);
        spawners.append(&mut self.spawning.custom_spawners);
        self.spawning.custom_spawners = spawners;
        true
    }

    /// `tickCustomSpawners(spawnEnemies)`.
    pub fn tick_custom_spawners(&mut self, spawn_enemies: bool) {
        let mut spawners = std::mem::take(&mut self.spawning.custom_spawners);
        for spawner in &mut spawners {
            spawner.tick(self, spawn_enemies);
        }
        // spawners installed during the tick (none in vanilla) go after the existing ones
        spawners.append(&mut self.spawning.custom_spawners);
        self.spawning.custom_spawners = spawners;
    }
}
