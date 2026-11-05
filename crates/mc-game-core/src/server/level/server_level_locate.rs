//! The level side of `/locate`: `ServerLevel.findClosestBiome3d`, `ChunkGenerator.findNearestMapStructure` over this
//! level's chunks (`LocateLevel`), and `ChunkMap.resendBiomesForChunks` for `/fillbiome`.

use super::chunk_holder::ChunkSlot;
use super::server_chunk_cache::ChunkFuture;
use super::ServerLevel;
use crate::world::level::levelgen::structure::locate::{LocateLevel, find_nearest_map_structure};
use mc_core::ident::Identifier;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_world::chunk::StructureData;
use mc_world::palette::BiomeId;
use mc_world::status::ChunkStatus;
use mc_worldgen::structure_placement::LocateResult;

impl ServerLevel {
    /// `ServerLevel.findClosestBiome3d(biomePredicate, origin, searchRadius, sampleResolutionHorizontal,
    /// sampleResolutionVertical)`.
    pub fn find_closest_biome_3d(
        &self,
        allowed: &dyn Fn(BiomeId) -> bool,
        origin: BlockPos,
        search_radius: i32,
        sample_resolution_horizontal: i32,
        sample_resolution_vertical: i32,
    ) -> Option<(BlockPos, BiomeId)> {
        let env = self.gen_env.clone()?;
        env.biome_source().find_closest_biome_3d(
            origin,
            search_radius,
            sample_resolution_horizontal,
            sample_resolution_vertical,
            allowed,
            &env.random_state,
            crate::world::level::BlockGetter::level_height(self),
        )
    }

    /// `serverLevel.getChunkSource().getGenerator().findNearestMapStructure(level, wanted, pos, radius, createReference)`.
    pub fn find_nearest_map_structure(&mut self, wanted: &[Identifier], pos: BlockPos, max_search_radius: i32, create_reference: bool) -> LocateResult {
        let env = self.gen_env.clone()?;
        find_nearest_map_structure(&env, &mut LevelLocate(self), wanted, pos, max_search_radius, create_reference)
    }

    /// `level.getChunk(x, z, ChunkStatus.STRUCTURE_STARTS)` handed to `f` as its structure data.
    fn with_structure_starts(&mut self, x: i32, z: i32, f: &mut dyn FnMut(&StructureData)) -> bool {
        let future = self.chunk_source.get_chunk_future_main_thread(x, z, ChunkStatus::StructureStarts, true);
        self.apply_chunk_events();
        if future == ChunkFuture::Unloaded {
            return false;
        }
        self.block_on_chunks(|cs| cs.future_result(future).is_some());
        if self.chunk_source.future_result(future) != Some(true) {
            return false;
        }
        match self.chunk_source.chunk_map.holders.holder(ChunkPos::pack_xz(x, z)).map(|h| &h.slot) {
            Some(ChunkSlot::Proto(chunk)) => f(&chunk.access.structures),
            Some(ChunkSlot::Level(chunk)) => f(&chunk.data.access.structures),
            _ => return false,
        }
        true
    }

    /// `ChunkMap.resendBiomesForChunks(chunks)`: every player tracking one of the (full) chunks gets one
    /// `ClientboundChunksBiomesPacket` with the biome data of its chunks.
    pub fn resend_biomes_for_chunks(&self, chunks: &[ChunkPos]) {
        use mc_protocol::packets::play::level::{ChunkBiomeData, ClientboundChunksBiomesPacket};
        let map = &self.chunk_source.chunk_map;
        let mut per_player: Vec<(crate::world::entity::EntityId, Vec<ChunkBiomeData>)> = Vec::new();
        for pos in chunks {
            let Some(ChunkSlot::Level(chunk)) = map.holders.holder(pos.pack()).map(|h| &h.slot) else { continue };
            let mut buffer = Vec::new();
            for section in &chunk.data.access.sections {
                section.biomes().write(&mut buffer).expect("biome containers serialize");
            }
            for player in map.get_players(*pos, false) {
                let data = ChunkBiomeData { pos: *pos, buffer: buffer.clone() };
                match per_player.iter_mut().find(|(id, _)| *id == player) {
                    Some((_, list)) => list.push(data),
                    None => per_player.push((player, vec![data])),
                }
            }
        }
        for (player, chunk_biome_data) in per_player {
            if let Some(p) = map.players.get(&player) {
                p.info.connection.send(&ClientboundChunksBiomesPacket { chunk_biome_data });
            }
        }
    }
}

/// `LocateLevel` over a level.
struct LevelLocate<'a>(&'a mut ServerLevel);

impl LocateLevel for LevelLocate<'_> {
    fn with_structure_data(&mut self, pos: ChunkPos, f: &mut dyn FnMut(&StructureData)) -> bool {
        self.0.with_structure_starts(pos.x, pos.z, f)
    }

    fn set_start_references(&mut self, pos: ChunkPos, structure_id: &str, references: i32) {
        let Some(holder) = self.0.chunk_source.chunk_map.holders.holder_mut(pos.pack()) else { return };
        match &mut holder.slot {
            ChunkSlot::Proto(chunk) => {
                if let Some(start) = chunk.access.structures.starts.get_mut(structure_id) {
                    start.put_int("references", references);
                }
                chunk.access.mark_unsaved();
            }
            ChunkSlot::Level(chunk) => {
                if let Some(start) = chunk.data.access.structures.starts.get_mut(structure_id) {
                    start.put_int("references", references);
                }
                chunk.data.access.mark_unsaved();
            }
            _ => {}
        }
    }
}
