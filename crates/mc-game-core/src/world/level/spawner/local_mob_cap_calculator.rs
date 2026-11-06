//! `net.minecraft.world.level.LocalMobCapCalculator`: the per-player mob counts that cap natural spawning around each
//! player (a chunk may spawn a category while any player close enough to it is below the category's per-chunk cap).

use super::mob_category::MobCategoryExt;
use crate::server::level::chunk_map::ChunkMap;
use crate::world::entity::EntityId;
use mc_core::pos::ChunkPos;
use mc_data::enums::MobCategory;
use std::collections::HashMap;

/// `LocalMobCapCalculator.MobCounts`.
#[derive(Default)]
struct MobCounts {
    counts: [i32; 8],
}

impl MobCounts {
    fn add(&mut self, category: MobCategory) {
        self.counts[category as usize] += 1;
    }

    fn can_spawn(&self, category: MobCategory) -> bool {
        self.counts[category as usize] < category.get_max_instances_per_chunk()
    }
}

/// `LocalMobCapCalculator`.
#[derive(Default)]
pub struct LocalMobCapCalculator {
    /// `playersNearChunk`.
    players_near_chunk: HashMap<i64, Vec<EntityId>>,
    /// `playerMobCounts`.
    player_mob_counts: HashMap<EntityId, MobCounts>,
}

impl LocalMobCapCalculator {
    pub fn new() -> LocalMobCapCalculator {
        LocalMobCapCalculator::default()
    }

    /// `getPlayersNear(pos)`: memoized `chunkMap.getPlayersCloseForSpawning(pos)`.
    fn get_players_near(&mut self, chunk_map: &mut ChunkMap, pos: ChunkPos) -> Vec<EntityId> {
        self.players_near_chunk
            .entry(pos.pack())
            .or_insert_with(|| chunk_map.get_players_close_for_spawning(pos))
            .clone()
    }

    /// `addMob(pos, category)`.
    pub fn add_mob(&mut self, chunk_map: &mut ChunkMap, pos: ChunkPos, category: MobCategory) {
        for player in self.get_players_near(chunk_map, pos) {
            self.player_mob_counts
                .entry(player)
                .or_default()
                .add(category);
        }
    }

    /// `canSpawn(category, pos)`.
    pub fn can_spawn(
        &mut self,
        chunk_map: &mut ChunkMap,
        category: MobCategory,
        pos: ChunkPos,
    ) -> bool {
        for player in self.get_players_near(chunk_map, pos) {
            match self.player_mob_counts.get(&player) {
                None => return true,
                Some(counts) if counts.can_spawn(category) => return true,
                Some(_) => {}
            }
        }
        false
    }
}
