//! Seam between the chunk map and the light engine (`ThreadedLevelLightEngine`): the operations the chunk status
//! pipeline needs. `mc-light`'s `ThreadedLevelLightEngine` is adapted to this trait when it lands on main; until then
//! [`UnlitLightEngine`] completes the light steps without computing any light.

use mc_core::pos::ChunkPos;
use mc_world::chunk::ChunkAccess;
use mc_world::data_layer::DataLayer;

/// Completion of a queued light task (`POST_UPDATE` futures of the threaded engine).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LightEvent {
    /// `initializeLight` finished for the chunk.
    Initialized(ChunkPos),
    /// `lightChunk` finished: the chunk is `lightCorrect`.
    Lit(ChunkPos),
    /// `waitForPendingTasks` barrier with the id returned by `wait_for_pending_tasks`.
    Barrier(u64),
}

pub trait ChunkLightEngine {
    fn initialize_light(&mut self, chunk: &ChunkAccess, lighted: bool);
    fn light_chunk(&mut self, pos: ChunkPos, lighted: bool);
    /// `updateChunkStatus(pos)` (chunk unloaded).
    fn update_chunk_status(&mut self, pos: ChunkPos);
    /// The load side of `SerializableChunkData.read`.
    fn load_chunk_light(&mut self, pos: ChunkPos, light: Vec<(i32, Option<DataLayer>, Option<DataLayer>)>);
    fn wait_for_pending_tasks(&mut self) -> u64;
    /// `hasLightWork()` / queued tasks.
    fn has_work(&self) -> bool;
    /// `runUpdate()`: process queued tasks (`chunk(x, z)` yields the chunk for lighting) and report completions.
    fn run_update<'a>(&mut self, chunk: &(dyn Fn(i32, i32) -> Option<&'a ChunkAccess> + 'a)) -> Vec<LightEvent>;
    /// `checkBlock(pos)`.
    fn check_block(&mut self, pos: mc_core::pos::BlockPos);
    /// `updateSectionStatus(pos, sectionEmpty)`.
    fn update_section_status(&mut self, pos: mc_core::pos::SectionPos, section_empty: bool);
    /// `getLayerListener(layer).getLightValue(pos)`.
    fn get_light_value(&self, sky: bool, pos: mc_core::pos::BlockPos) -> i32;
    /// `getRawBrightness(pos, skyDampen)`.
    fn get_raw_brightness(&self, pos: mc_core::pos::BlockPos, sky_dampen: i32) -> i32;
    /// `ClientboundLightUpdatePacketData` for the changed sections (`None` when the engine has no light data).
    fn light_update_packet_data(
        &self,
        pos: ChunkPos,
        sky_changed: &std::collections::HashSet<i32>,
        block_changed: &std::collections::HashSet<i32>,
    ) -> Option<mc_protocol::packets::play::level::ClientboundLightUpdatePacketData>;
    /// `(blockLight, skyLight)` of a section for saving (`copyOf`).
    fn section_light(&self, pos: ChunkPos, section_y: i32) -> (Option<DataLayer>, Option<DataLayer>);
}

/// Completes light tasks immediately and stores no light data.
#[derive(Default)]
pub struct UnlitLightEngine {
    events: Vec<LightEvent>,
    next_barrier: u64,
}

impl ChunkLightEngine for UnlitLightEngine {
    fn initialize_light(&mut self, chunk: &ChunkAccess, _lighted: bool) {
        self.events.push(LightEvent::Initialized(chunk.pos));
    }
    fn light_chunk(&mut self, pos: ChunkPos, _lighted: bool) {
        self.events.push(LightEvent::Lit(pos));
    }
    fn update_chunk_status(&mut self, _pos: ChunkPos) {}
    fn load_chunk_light(&mut self, _pos: ChunkPos, _light: Vec<(i32, Option<DataLayer>, Option<DataLayer>)>) {}
    fn wait_for_pending_tasks(&mut self) -> u64 {
        let id = self.next_barrier;
        self.next_barrier += 1;
        self.events.push(LightEvent::Barrier(id));
        id
    }
    fn has_work(&self) -> bool {
        !self.events.is_empty()
    }
    fn run_update<'a>(&mut self, _chunk: &(dyn Fn(i32, i32) -> Option<&'a ChunkAccess> + 'a)) -> Vec<LightEvent> {
        std::mem::take(&mut self.events)
    }
    fn section_light(&self, _pos: ChunkPos, _section_y: i32) -> (Option<DataLayer>, Option<DataLayer>) {
        (None, None)
    }
    fn check_block(&mut self, _pos: mc_core::pos::BlockPos) {}
    fn update_section_status(&mut self, _pos: mc_core::pos::SectionPos, _section_empty: bool) {}
    fn get_light_value(&self, _sky: bool, _pos: mc_core::pos::BlockPos) -> i32 {
        0
    }
    fn get_raw_brightness(&self, _pos: mc_core::pos::BlockPos, _sky_dampen: i32) -> i32 {
        0
    }
    fn light_update_packet_data(
        &self,
        _pos: ChunkPos,
        _sky_changed: &std::collections::HashSet<i32>,
        _block_changed: &std::collections::HashSet<i32>,
    ) -> Option<mc_protocol::packets::play::level::ClientboundLightUpdatePacketData> {
        None
    }
}
