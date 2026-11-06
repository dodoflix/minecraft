//! `MinecraftServer.setInitialSpawn` (the world spawn of a new world) and the level glue `PlayerSpawnFinder` needs.

use super::MinecraftServer;
use super::level_load_listener::Stage;
use super::spawn_search::spawn_spiral;
use crate::server::level::ServerLevel;
use crate::server::level::player_spawn_finder::{SpawnSearchLevel, get_spawn_pos_in_chunk};
use crate::world::level::{BlockGetter, LevelReader};
use mc_core::pos::{BlockPos, ChunkPos};
use mc_world::heightmap::HeightmapType;
use mc_world::level_storage::RespawnData;

impl SpawnSearchLevel for ServerLevel {
    fn has_ceiling(&self) -> bool {
        self.has_ceiling
    }

    fn generator_spawn_height(&self) -> i32 {
        self.chunk_source
            .generator()
            .get_spawn_height(self.level_height())
    }

    /// `LevelChunk.getHeight(type, x & 15, z & 15)`: one below the first free block of the column.
    fn chunk_height(&self, ty: HeightmapType, x: i32, z: i32) -> i32 {
        match self
            .chunk_now(x >> 4, z >> 4)
            .and_then(|chunk| chunk.data.access.heightmaps.get(&ty))
        {
            Some(heightmap) => heightmap.get_first_available(x & 15, z & 15) - 1,
            None => self.get_min_y() - 1,
        }
    }
}

impl MinecraftServer {
    /// `setInitialSpawn(level, levelData, spawnBonusChest, isDebug, listener)` for the overworld.
    pub(super) fn set_initial_spawn(
        &mut self,
        level: &mut ServerLevel,
        spawn_bonus_chest: bool,
        is_debug: bool,
    ) {
        let dimension = level.dimension.clone();
        if is_debug {
            self.world_data.lock().respawn_data = RespawnData {
                dimension,
                pos: BlockPos::ZERO.above_n(80),
                yaw: 0.0,
                pitch: 0.0,
            };
            return;
        }
        let spawn_chunk = level.chunk_source.generator().get_origin();
        self.level_load_listener.start(Stage::PrepareGlobalSpawn, 0);
        self.level_load_listener
            .update_focus(&dimension, spawn_chunk);
        let mut height = level
            .chunk_source
            .generator()
            .get_spawn_height(level.level_height());
        if height < level.get_min_y() {
            let world_position = BlockPos::new(
                spawn_chunk.get_min_block_x(),
                0,
                spawn_chunk.get_min_block_z(),
            );
            level.chunk_blocking_mut(world_position.x() >> 4, world_position.z() >> 4);
            height = level.get_height_at(
                HeightmapType::WorldSurface,
                world_position.x() + 8,
                world_position.z() + 8,
            );
        }
        let origin_pos = BlockPos::new(
            spawn_chunk.get_min_block_x() + 8,
            height,
            spawn_chunk.get_min_block_z() + 8,
        );
        self.world_data.lock().respawn_data = RespawnData {
            dimension: dimension.clone(),
            pos: origin_pos,
            yaw: 0.0,
            pitch: 0.0,
        };

        for (x_chunk_offset, z_chunk_offset) in spawn_spiral() {
            let chunk_pos = ChunkPos::new(
                spawn_chunk.x + x_chunk_offset,
                spawn_chunk.z + z_chunk_offset,
            );
            level.chunk_blocking_mut(chunk_pos.x, chunk_pos.z);
            if let Some(tested_pos) = get_spawn_pos_in_chunk(level, chunk_pos) {
                self.world_data.lock().respawn_data = RespawnData {
                    dimension: dimension.clone(),
                    pos: tested_pos,
                    yaw: 0.0,
                    pitch: 0.0,
                };
                break;
            }
        }

        if spawn_bonus_chest {
            // `MiscOverworldFeatures.BONUS_CHEST` placement is not implemented: configured feature placement lives in
            // `mc-game-levelgen`, which `mc-game-core` cannot call.
            log::warn!(
                "The bonus chest cannot be placed: configured feature placement is not available"
            );
        }
        self.level_load_listener.finish(Stage::PrepareGlobalSpawn);
    }
}
