//! `LevelLightEngine` (+ `LightEventListener`, `LayerLightEventListener`).

use crate::chunk_source::LightChunkGetter;
use crate::engine::LightEngine;
use crate::storage::{LayerReader, LightLayer, SectionType};
use mc_core::pos::{BlockPos, ChunkPos, SectionPos};
use mc_world::chunk::LevelHeight;
use mc_world::chunk_packet::{BitSet, LightUpdatePacketData};
use mc_world::data_layer::DataLayer;
use std::sync::Arc;

/// `LevelLightEngine.LIGHT_SECTION_PADDING`.
pub const LIGHT_SECTION_PADDING: i32 = 1;

pub struct LevelLightEngine {
    level_height: LevelHeight,
    block_engine: Option<LightEngine>,
    sky_engine: Option<LightEngine>,
}

impl LevelLightEngine {
    /// `new LevelLightEngine(chunkSource, hasBlockLight, hasSkyLight)`; `level_height` is `chunkSource.getLevel()`.
    pub fn new(level_height: LevelHeight, has_block_light: bool, has_sky_light: bool) -> Self {
        LevelLightEngine {
            level_height,
            block_engine: has_block_light.then(|| LightEngine::new(LightLayer::Block, level_height)),
            sky_engine: has_sky_light.then(|| LightEngine::new(LightLayer::Sky, level_height)),
        }
    }

    pub fn level_height(&self) -> LevelHeight {
        self.level_height
    }

    fn engine(&self, layer: LightLayer) -> Option<&LightEngine> {
        match layer {
            LightLayer::Block => self.block_engine.as_ref(),
            LightLayer::Sky => self.sky_engine.as_ref(),
        }
    }

    /// `checkBlock(pos)`.
    pub fn check_block(&mut self, pos: BlockPos) {
        for engine in [&mut self.block_engine, &mut self.sky_engine].into_iter().flatten() {
            engine.check_block(pos);
        }
    }

    pub fn has_light_work(&self) -> bool {
        self.sky_engine.as_ref().is_some_and(LightEngine::has_light_work) || self.block_engine.as_ref().is_some_and(LightEngine::has_light_work)
    }

    /// `runLightUpdates()` (block layer first, then sky).
    pub fn run_light_updates(&mut self, chunks: &dyn LightChunkGetter) -> i32 {
        let mut count = 0;
        for engine in [&mut self.block_engine, &mut self.sky_engine].into_iter().flatten() {
            count += engine.run_light_updates(chunks);
        }
        count
    }

    /// `updateSectionStatus(pos, sectionEmpty)`.
    pub fn update_section_status(&mut self, pos: SectionPos, section_empty: bool) {
        for engine in [&mut self.block_engine, &mut self.sky_engine].into_iter().flatten() {
            engine.update_section_status(pos, section_empty);
        }
    }

    /// `updateSectionStatus(BlockPos, sectionEmpty)`.
    pub fn update_section_status_at(&mut self, pos: BlockPos, section_empty: bool) {
        self.update_section_status(SectionPos::of_block_pos(&pos), section_empty);
    }

    /// `setLightEnabled(pos, enable)`.
    pub fn set_light_enabled(&mut self, chunks: &dyn LightChunkGetter, pos: ChunkPos, enable: bool) {
        for engine in [&mut self.block_engine, &mut self.sky_engine].into_iter().flatten() {
            engine.set_light_enabled(chunks, pos, enable);
        }
    }

    /// `propagateLightSources(pos)`.
    pub fn propagate_light_sources(&mut self, chunks: &dyn LightChunkGetter, pos: ChunkPos) {
        for engine in [&mut self.block_engine, &mut self.sky_engine].into_iter().flatten() {
            engine.propagate_light_sources(chunks, pos);
        }
    }

    /// `queueSectionData(layer, pos, data)`.
    pub fn queue_section_data(&mut self, layer: LightLayer, pos: SectionPos, data: Option<DataLayer>) {
        let engine = match layer {
            LightLayer::Block => self.block_engine.as_mut(),
            LightLayer::Sky => self.sky_engine.as_mut(),
        };
        if let Some(engine) = engine {
            engine.queue_section_data(pos, data);
        }
    }

    /// `retainData(pos, retain)`.
    pub fn retain_data(&mut self, pos: ChunkPos, retain: bool) {
        for engine in [&mut self.block_engine, &mut self.sky_engine].into_iter().flatten() {
            engine.retain_data(pos, retain);
        }
    }

    /// `getLayerListener(layer).getDataLayerData(pos)`: `None` for a missing layer engine (the dummy listener).
    pub fn get_data_layer_data(&self, layer: LightLayer, pos: SectionPos) -> Option<Arc<DataLayer>> {
        self.engine(layer)?.get_data_layer_data(pos)
    }

    /// `getLayerListener(layer).getLightValue(pos)`.
    pub fn get_light_value(&self, layer: LightLayer, pos: BlockPos) -> i32 {
        self.engine(layer).map_or(0, |e| e.get_light_value(pos))
    }

    /// `getRawBrightness(pos, skyDampen)`.
    pub fn get_raw_brightness(&self, pos: BlockPos, sky_dampen: i32) -> i32 {
        let sky_light = self.sky_engine.as_ref().map_or(0, |e| e.get_light_value(pos) - sky_dampen);
        let block_light = self.block_engine.as_ref().map_or(0, |e| e.get_light_value(pos));
        block_light.max(sky_light)
    }

    /// `lightOnInColumn(sectionZeroNode)`.
    pub fn light_on_in_column(&self, section_zero_node: i64) -> bool {
        match &self.block_engine {
            None => true,
            Some(block) => block.storage.light_on_in_column(section_zero_node) && self.sky_engine.as_ref().is_none_or(|sky| sky.storage.light_on_in_column(section_zero_node)),
        }
    }

    pub fn get_debug_section_type(&self, layer: LightLayer, pos: SectionPos) -> SectionType {
        self.engine(layer).map_or(SectionType::Empty, |e| e.get_debug_section_type(pos))
    }

    /// `getDebugData(layer, pos)`.
    pub fn get_debug_data(&self, layer: LightLayer, pos: SectionPos) -> &'static str {
        match self.engine(layer) {
            Some(e) => e.get_debug_section_type(pos).display(),
            None => "n/a",
        }
    }

    pub fn get_light_section_count(&self) -> i32 {
        self.level_height.sections_count() as i32 + 2 * LIGHT_SECTION_PADDING
    }

    pub fn get_min_light_section(&self) -> i32 {
        self.level_height.min_section_y() - LIGHT_SECTION_PADDING
    }

    pub fn get_max_light_section(&self) -> i32 {
        self.get_min_light_section() + self.get_light_section_count()
    }

    /// Sections whose light changed since the last call, per layer (vanilla `onLightUpdate(layer, pos)` callbacks).
    pub fn take_light_updates(&mut self) -> Vec<(LightLayer, SectionPos)> {
        let mut out = Vec::new();
        for engine in [&mut self.block_engine, &mut self.sky_engine].into_iter().flatten() {
            let layer = engine.layer();
            out.extend(engine.take_light_updates().into_iter().map(|s| (layer, s)));
        }
        out
    }

    /// A cloneable, thread-safe handle for reading the published light data (vanilla reads `visibleSectionData`
    /// from other threads while the engine runs).
    pub fn reader(&self) -> LightReader {
        LightReader {
            level_height: self.level_height,
            block: self.block_engine.as_ref().map(LightEngine::reader),
            sky: self.sky_engine.as_ref().map(LightEngine::reader),
        }
    }

    /// `new ClientboundLightUpdatePacketData(chunkPos, lightEngine, skyFilter, blockFilter)`.
    pub fn light_update_packet_data(&self, chunk_pos: ChunkPos, sky_changed_filter: Option<&BitSet>, block_changed_filter: Option<&BitSet>) -> LightUpdatePacketData {
        self.reader().light_update_packet_data(chunk_pos, sky_changed_filter, block_changed_filter)
    }

    /// The `light(sectionY) -> (blockLight, skyLight)` accessor `SerializableChunkData::copy_of` expects.
    pub fn chunk_light(&self, pos: ChunkPos) -> impl Fn(i32) -> (Option<DataLayer>, Option<DataLayer>) + '_ {
        move |section_y| {
            let section = SectionPos::of_chunk(&pos, section_y);
            let get = |layer| self.get_data_layer_data(layer, section).map(|l| (*l).clone());
            (get(LightLayer::Block), get(LightLayer::Sky))
        }
    }

    /// The load side of `SerializableChunkData.read`: queues the saved layers of a chunk (`light` as returned in
    /// `LoadedChunkData::light`, sky layers already dropped for dimensions without sky light).
    pub fn load_chunk_light(&mut self, pos: ChunkPos, light: Vec<(i32, Option<DataLayer>, Option<DataLayer>)>) {
        let mut loaded_any_light = false;
        for (section_y, block_light, sky_light) in light {
            if block_light.is_none() && sky_light.is_none() {
                continue;
            }
            if !loaded_any_light {
                self.retain_data(pos, true);
                loaded_any_light = true;
            }
            let section_pos = SectionPos::of_chunk(&pos, section_y);
            if let Some(block_light) = block_light {
                self.queue_section_data(LightLayer::Block, section_pos, Some(block_light));
            }
            if let Some(sky_light) = sky_light {
                self.queue_section_data(LightLayer::Sky, section_pos, Some(sky_light));
            }
        }
    }
}

/// Read-only view of the published light data (`LevelLightEngine.getRawBrightness`, `getLayerListener(..)` queries).
#[derive(Clone)]
pub struct LightReader {
    level_height: LevelHeight,
    block: Option<LayerReader>,
    sky: Option<LayerReader>,
}

impl LightReader {
    fn layer(&self, layer: LightLayer) -> Option<&LayerReader> {
        match layer {
            LightLayer::Block => self.block.as_ref(),
            LightLayer::Sky => self.sky.as_ref(),
        }
    }

    pub fn get_data_layer_data(&self, layer: LightLayer, pos: SectionPos) -> Option<Arc<DataLayer>> {
        self.layer(layer)?.get_data_layer_data(pos)
    }

    pub fn get_light_value(&self, layer: LightLayer, pos: BlockPos) -> i32 {
        self.layer(layer).map_or(0, |r| r.get_light_value(pos))
    }

    pub fn get_raw_brightness(&self, pos: BlockPos, sky_dampen: i32) -> i32 {
        let sky_light = self.sky.as_ref().map_or(0, |r| r.get_light_value(pos) - sky_dampen);
        let block_light = self.block.as_ref().map_or(0, |r| r.get_light_value(pos));
        block_light.max(sky_light)
    }

    pub fn get_light_section_count(&self) -> i32 {
        self.level_height.sections_count() as i32 + 2 * LIGHT_SECTION_PADDING
    }

    pub fn get_min_light_section(&self) -> i32 {
        self.level_height.min_section_y() - LIGHT_SECTION_PADDING
    }

    /// `new ClientboundLightUpdatePacketData(chunkPos, lightEngine, skyFilter, blockFilter)`.
    pub fn light_update_packet_data(&self, chunk_pos: ChunkPos, sky_changed_filter: Option<&BitSet>, block_changed_filter: Option<&BitSet>) -> LightUpdatePacketData {
        let min_light_section = self.get_min_light_section();
        let layers = |layer| -> Vec<Option<Arc<DataLayer>>> {
            (0..self.get_light_section_count()).map(|i| self.get_data_layer_data(layer, SectionPos::of_chunk(&chunk_pos, min_light_section + i))).collect()
        };
        let (sky, block) = (layers(LightLayer::Sky), layers(LightLayer::Block));
        let sky: Vec<Option<&DataLayer>> = sky.iter().map(|l| l.as_deref()).collect();
        let block: Vec<Option<&DataLayer>> = block.iter().map(|l| l.as_deref()).collect();
        LightUpdatePacketData::new(&sky, &block, sky_changed_filter, block_changed_filter)
    }
}
