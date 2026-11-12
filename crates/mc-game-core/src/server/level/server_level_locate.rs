//! The level side of `/locate`: `ServerLevel.findClosestBiome3d`, `ChunkGenerator.findNearestMapStructure` over this
//! level's chunks (`LocateLevel`), and `ChunkMap.resendBiomesForChunks` for `/fillbiome`.

use super::ServerLevel;
use super::chunk_holder::ChunkSlot;
use super::server_chunk_cache::ChunkFuture;
use crate::advancements::predicates::location_predicate::LoadedChunks;
use crate::world::level::levelgen::GenEnv;
use crate::world::level::levelgen::structure::StructureManager;
use crate::world::level::levelgen::structure::generation;
use crate::world::level::levelgen::structure::locate::{LocateLevel, find_nearest_map_structure};
use crate::world::level::levelgen::structure::structure_start::MAX_REFERENCES;
use mc_core::ident::Identifier;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_world::chunk::{ProtoChunk, StructureData};
use mc_world::palette::BiomeId;
use mc_world::serial::{
    SerializableChunkData, unpack_structure_references, unpack_structure_starts,
};
use mc_world::status::ChunkStatus;
use mc_world::upgrade_data::UpgradeData;
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
    pub fn find_nearest_map_structure(
        &mut self,
        wanted: &[Identifier],
        pos: BlockPos,
        max_search_radius: i32,
        create_reference: bool,
    ) -> LocateResult {
        self.apply_pending_start_references();
        let env = self.gen_env.clone()?;
        find_nearest_map_structure(
            &env,
            &mut LevelLocate(self),
            wanted,
            pos,
            max_search_radius,
            create_reference,
        )
    }

    /// [`Self::find_nearest_map_structure`] for callers that only hold a shared level (loot functions): a chunk's structure
    /// data is read from the loaded chunk, else from its region file, else generated without being stored (what loading the
    /// chunk to `STRUCTURE_STARTS` would produce). The reference counters it raises wait in `pending_start_references`
    /// until [`Self::apply_pending_start_references`] stores them in the chunks.
    pub fn find_nearest_map_structure_shared(
        &self,
        wanted: &[Identifier],
        pos: BlockPos,
        max_search_radius: i32,
        create_reference: bool,
    ) -> LocateResult {
        let env = self.gen_env.clone()?;
        find_nearest_map_structure(
            &env,
            &mut SharedLocate(self),
            wanted,
            pos,
            max_search_radius,
            create_reference,
        )
    }

    /// `ExplorationMapFunction.claimStructureAt(level, pos)`: references the valid, still unreferenced start of one of the
    /// `wanted` structures that contains `pos`.
    pub fn claim_structure_at(&self, wanted: &[Identifier], pos: BlockPos) {
        let Some(env) = self.gen_env.as_ref() else {
            return;
        };
        let found =
            StructureManager::new(env)
                .get_structure_at_set(&LoadedChunks(self), pos, &|id| wanted.contains(id));
        let Some(start) = found else {
            return;
        };
        let start = start.lock();
        let Some(id) = start.structure_id() else {
            return;
        };
        let chunk_pos = start.get_chunk_pos();
        let key = (chunk_pos.pack(), id.to_string());
        let mut pending = self.pending_start_references.lock();
        let references = pending.get(&key).copied().unwrap_or(start.get_references());
        // `isValid() && canBeReferenced()`, then `addReference(start)`
        if start.is_valid() && references < MAX_REFERENCES {
            env.structure_check.increment_reference(chunk_pos, id);
            pending.insert(key, references + 1);
        }
    }

    /// Stores the reference counters raised through a shared level into the starts of their chunks (loading the chunks
    /// to `STRUCTURE_STARTS` like the exclusive search does).
    pub fn apply_pending_start_references(&mut self) {
        let pending = std::mem::take(&mut *self.pending_start_references.lock());
        for ((key, id), references) in pending {
            let pos = ChunkPos::unpack(key);
            if self.with_structure_starts(pos.x, pos.z, &mut |_| {}) {
                LevelLocate(self).set_start_references(pos, &id, references);
            } else {
                self.pending_start_references
                    .lock()
                    .insert((key, id), references);
            }
        }
    }

    /// The structure data of a chunk at `STRUCTURE_STARTS` without touching the chunk map (see
    /// [`Self::find_nearest_map_structure_shared`]).
    fn structure_data_without_loading(&self, env: &GenEnv, pos: ChunkPos) -> StructureData {
        let map = &self.chunk_source.chunk_map;
        if let Some(holder) = map.holders.holder(pos.pack())
            && holder
                .latest_chunk_status()
                .is_some_and(|status| status.is_or_after(ChunkStatus::StructureStarts))
        {
            match &holder.slot {
                ChunkSlot::Proto(chunk) => return chunk.access.structures.clone(),
                ChunkSlot::Level(chunk) => return chunk.data.access.structures.clone(),
                _ => {}
            }
        }
        let is_known = |id: &str| {
            Identifier::try_parse(id).is_some_and(|id| env.structures.get(&id).is_some())
        };
        match map.io.read_blocking(pos) {
            Ok(Some(tag))
                if SerializableChunkData::get_chunk_status_from_tag(Some(&tag))
                    .is_or_after(ChunkStatus::StructureStarts) =>
            {
                let structures = tag.get_compound_or_empty("structures");
                return StructureData {
                    starts: unpack_structure_starts(structures, &is_known),
                    references: unpack_structure_references(structures, pos, &is_known),
                };
            }
            Ok(_) => {}
            Err(e) => log::error!("Failed to read chunk {pos}: {e}"),
        }
        let context = &map.gen_context;
        let mut chunk = ProtoChunk::new(
            pos,
            UpgradeData::EMPTY,
            context.level_height,
            &context.factory,
            None,
        );
        generation::create_structures(env, &mut chunk);
        chunk.access.structures
    }

    /// `level.getChunk(x, z, ChunkStatus.STRUCTURE_STARTS)` handed to `f` as its structure data.
    fn with_structure_starts(&mut self, x: i32, z: i32, f: &mut dyn FnMut(&StructureData)) -> bool {
        let future = self.chunk_source.get_chunk_future_main_thread(
            x,
            z,
            ChunkStatus::StructureStarts,
            true,
        );
        self.apply_chunk_events();
        if future == ChunkFuture::Unloaded {
            return false;
        }
        self.block_on_chunks(|cs| cs.future_result(future).is_some());
        if self.chunk_source.future_result(future) != Some(true) {
            return false;
        }
        match self
            .chunk_source
            .chunk_map
            .holders
            .holder(ChunkPos::pack_xz(x, z))
            .map(|h| &h.slot)
        {
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
            let Some(ChunkSlot::Level(chunk)) = map.holders.holder(pos.pack()).map(|h| &h.slot)
            else {
                continue;
            };
            let mut buffer = Vec::new();
            for section in &chunk.data.access.sections {
                section
                    .biomes()
                    .write(&mut buffer)
                    .expect("biome containers serialize");
            }
            for player in map.get_players(*pos, false) {
                let data = ChunkBiomeData {
                    pos: *pos,
                    buffer: buffer.clone(),
                };
                match per_player.iter_mut().find(|(id, _)| *id == player) {
                    Some((_, list)) => list.push(data),
                    None => per_player.push((player, vec![data])),
                }
            }
        }
        for (player, chunk_biome_data) in per_player {
            if let Some(p) = map.players.get(&player) {
                p.info
                    .connection
                    .send(&ClientboundChunksBiomesPacket { chunk_biome_data });
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
        let Some(holder) = self.0.chunk_source.chunk_map.holders.holder_mut(pos.pack()) else {
            return;
        };
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

/// `LocateLevel` over a shared level (see [`ServerLevel::find_nearest_map_structure_shared`]).
struct SharedLocate<'a>(&'a ServerLevel);

impl LocateLevel for SharedLocate<'_> {
    fn with_structure_data(&mut self, pos: ChunkPos, f: &mut dyn FnMut(&StructureData)) -> bool {
        let Some(env) = self.0.gen_env.as_ref() else {
            return false;
        };
        let mut data = self.0.structure_data_without_loading(env, pos);
        for ((key, id), references) in self.0.pending_start_references.lock().iter() {
            if *key == pos.pack()
                && let Some(start) = data.starts.get_mut(id)
            {
                start.put_int("references", *references);
            }
        }
        f(&data);
        true
    }

    fn set_start_references(&mut self, pos: ChunkPos, structure_id: &str, references: i32) {
        self.0
            .pending_start_references
            .lock()
            .insert((pos.pack(), structure_id.to_owned()), references);
    }
}
