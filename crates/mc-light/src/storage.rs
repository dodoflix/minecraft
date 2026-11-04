//! `DataLayerStorageMap`, `LayerLightSectionStorage` and the sky/block specializations
//! (`SkyLightSectionStorage`, `BlockLightSectionStorage`) in one type.

use crate::long_set::LongOpenHashSet;
use mc_core::direction::Direction;
use mc_core::pos::{BlockPos, SectionPos};
use mc_world::data_layer::DataLayer;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};

/// `LightLayer`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum LightLayer {
    Sky,
    Block,
}

/// `LayerLightSectionStorage.SectionType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SectionType {
    Empty,
    LightOnly,
    LightAndData,
}

impl SectionType {
    pub fn display(self) -> &'static str {
        match self {
            SectionType::Empty => "2",
            SectionType::LightOnly => "1",
            SectionType::LightAndData => "0",
        }
    }
}

/// `LayerLightSectionStorage.SectionState`: bit 5 = has data, low 5 bits = neighbor count (0..=26).
mod section_state {
    pub fn has_data_set(state: u8, has_data: bool) -> u8 {
        if has_data { state | 32 } else { state & !32 }
    }

    pub fn neighbor_count_set(state: u8, count: i32) -> u8 {
        assert!((0..=26).contains(&count), "Neighbor count was not within range [0; 26]");
        state & !31 | (count as u8 & 31)
    }

    pub fn has_data(state: u8) -> bool {
        state & 32 != 0
    }

    pub fn neighbor_count(state: u8) -> i32 {
        i32::from(state & 31)
    }
}

/// `DataLayerStorageMap` / `BlockDataLayerStorageMap` / `SkyDataLayerStorageMap`. Layers are shared copy-on-write
/// handles, so `clone()` (vanilla `copy()`) never duplicates light data.
#[derive(Clone, Debug)]
pub struct DataLayerStorageMap {
    map: HashMap<i64, Arc<DataLayer>>,
    /// Sky only: lowest section Y with data (`Integer.MAX_VALUE` when none).
    current_lowest_y: i32,
    /// Sky only: per column zero node, one above the highest section holding a layer.
    top_sections: HashMap<i64, i32>,
}

impl DataLayerStorageMap {
    fn new() -> Self {
        DataLayerStorageMap { map: HashMap::new(), current_lowest_y: i32::MAX, top_sections: HashMap::new() }
    }

    pub fn get_layer(&self, section_node: i64) -> Option<&Arc<DataLayer>> {
        self.map.get(&section_node)
    }

    fn get_layer_mut(&mut self, section_node: i64) -> Option<&mut DataLayer> {
        self.map.get_mut(&section_node).map(Arc::make_mut)
    }

    fn top_section(&self, zero_node: i64) -> i32 {
        self.top_sections.get(&zero_node).copied().unwrap_or(self.current_lowest_y)
    }
}

/// The part of a [`LayerLightSectionStorage`] other threads read: the published section map and the queued
/// (not yet applied) layers.
#[derive(Clone)]
pub struct LayerReader {
    layer: LightLayer,
    visible: Arc<RwLock<Arc<DataLayerStorageMap>>>,
    queued: Arc<Mutex<HashMap<i64, Arc<DataLayer>>>>,
}

impl LayerReader {
    /// `getDataLayerData(sectionNode)`: the queued layer if any, else the published one.
    pub fn get_data_layer_data(&self, section: SectionPos) -> Option<Arc<DataLayer>> {
        let node = section.as_long();
        if let Some(layer) = self.queued.lock().expect("queued light sections").get(&node) {
            return Some(layer.clone());
        }
        self.visible.read().expect("visible light map").get_layer(node).cloned()
    }

    /// `getLightValue(pos)` on the published data.
    pub fn get_light_value(&self, pos: BlockPos) -> i32 {
        let map = self.visible.read().expect("visible light map").clone();
        light_value(self.layer, &map, pos.as_long(), false, |_| false)
    }
}

fn layer_at(map: &DataLayerStorageMap, block_node: i64) -> i32 {
    // callers guarantee the layer exists
    let section = SectionPos::block_to_section(block_node);
    map.map[&section].get(
        SectionPos::section_relative(BlockPos::get_x(block_node)),
        SectionPos::section_relative(BlockPos::get_y(block_node)),
        SectionPos::section_relative(BlockPos::get_z(block_node)),
    )
}

/// `BlockLightSectionStorage.getLightValue` / `SkyLightSectionStorage.getLightValue(blockNode, updating)`.
/// `light_on_in_section` is only consulted for sky light with `updating`.
fn light_value(layer: LightLayer, sections: &DataLayerStorageMap, block_node: i64, updating: bool, light_on_in_section: impl Fn(i64) -> bool) -> i32 {
    let mut section_node = SectionPos::block_to_section(block_node);
    match layer {
        LightLayer::Block => sections.map.get(&section_node).map_or(0, |l| {
            l.get(
                SectionPos::section_relative(BlockPos::get_x(block_node)),
                SectionPos::section_relative(BlockPos::get_y(block_node)),
                SectionPos::section_relative(BlockPos::get_z(block_node)),
            )
        }),
        LightLayer::Sky => {
            let mut block_node = block_node;
            let mut section_y = SectionPos::y_from_long(section_node);
            let top_section = sections.top_section(SectionPos::get_zero_node(section_node));
            if top_section != sections.current_lowest_y && section_y < top_section {
                let mut layer = sections.map.get(&section_node);
                if layer.is_none() {
                    block_node = BlockPos::get_flat_index(block_node);
                    while layer.is_none() {
                        section_y += 1;
                        if section_y >= top_section {
                            return 15;
                        }
                        section_node = SectionPos::offset_long_direction(section_node, Direction::Up);
                        layer = sections.map.get(&section_node);
                    }
                }
                layer.expect("found above").get(
                    SectionPos::section_relative(BlockPos::get_x(block_node)),
                    SectionPos::section_relative(BlockPos::get_y(block_node)),
                    SectionPos::section_relative(BlockPos::get_z(block_node)),
                )
            } else if updating && !light_on_in_section(section_node) {
                0
            } else {
                15
            }
        }
    }
}

pub struct LayerLightSectionStorage {
    layer: LightLayer,
    section_states: HashMap<i64, u8>,
    columns_with_sources: HashSet<i64>,
    visible: Arc<RwLock<Arc<DataLayerStorageMap>>>,
    updating: DataLayerStorageMap,
    changed_sections: HashSet<i64>,
    sections_affected_by_light_updates: LongOpenHashSet,
    queued_sections: Arc<Mutex<HashMap<i64, Arc<DataLayer>>>>,
    columns_to_retain_queued_data_for: HashSet<i64>,
    to_remove: HashSet<i64>,
    has_inconsistencies: bool,
}

impl LayerLightSectionStorage {
    pub fn new(layer: LightLayer) -> Self {
        let updating = DataLayerStorageMap::new();
        LayerLightSectionStorage {
            layer,
            section_states: HashMap::new(),
            columns_with_sources: HashSet::new(),
            visible: Arc::new(RwLock::new(Arc::new(updating.clone()))),
            updating,
            changed_sections: HashSet::new(),
            sections_affected_by_light_updates: LongOpenHashSet::default(),
            queued_sections: Arc::new(Mutex::new(HashMap::new())),
            columns_to_retain_queued_data_for: HashSet::new(),
            to_remove: HashSet::new(),
            has_inconsistencies: false,
        }
    }

    pub fn reader(&self) -> LayerReader {
        LayerReader { layer: self.layer, visible: self.visible.clone(), queued: self.queued_sections.clone() }
    }

    pub fn storing_light_for_section(&self, section_node: i64) -> bool {
        self.updating.map.contains_key(&section_node)
    }

    /// `getDataLayerToWrite`.
    pub fn get_data_layer_to_write(&mut self, section_node: i64) -> Option<&mut DataLayer> {
        if !self.updating.map.contains_key(&section_node) {
            return None;
        }
        self.changed_sections.insert(section_node);
        self.updating.get_layer_mut(section_node)
    }

    pub fn get_data_layer_data(&self, section_node: i64) -> Option<Arc<DataLayer>> {
        if let Some(layer) = self.queued_sections.lock().expect("queued light sections").get(&section_node) {
            return Some(layer.clone());
        }
        self.visible.read().expect("visible light map").get_layer(section_node).cloned()
    }

    /// `getLightValue(blockNode)` on the published data.
    pub fn get_light_value(&self, block_node: i64) -> i32 {
        let map = self.visible.read().expect("visible light map").clone();
        light_value(self.layer, &map, block_node, false, |_| false)
    }

    /// `SkyLightSectionStorage.getLightValue(blockNode, updating = true)`; only used by tests/debugging.
    pub fn get_updating_light_value(&self, block_node: i64) -> i32 {
        light_value(self.layer, &self.updating, block_node, true, |s| self.light_on_in_section(s))
    }

    pub fn get_stored_level(&self, block_node: i64) -> i32 {
        layer_at(&self.updating, block_node)
    }

    pub fn set_stored_level(&mut self, block_node: i64, level: i32) {
        let section_node = SectionPos::block_to_section(block_node);
        self.changed_sections.insert(section_node);
        let layer = self.updating.get_layer_mut(section_node).expect("storing light for section");
        layer.set(
            SectionPos::section_relative(BlockPos::get_x(block_node)),
            SectionPos::section_relative(BlockPos::get_y(block_node)),
            SectionPos::section_relative(BlockPos::get_z(block_node)),
            level,
        );
        let affected = &mut self.sections_affected_by_light_updates;
        SectionPos::around_and_at_block_pos_long(block_node, |s| {
            affected.add(s);
        });
    }

    fn mark_section_and_neighbors_as_affected(&mut self, section_node: i64) {
        let (x, y, z) = (SectionPos::x_from_long(section_node), SectionPos::y_from_long(section_node), SectionPos::z_from_long(section_node));
        for offset_z in -1..=1 {
            for offset_x in -1..=1 {
                for offset_y in -1..=1 {
                    self.sections_affected_by_light_updates.add(SectionPos::as_long_xyz(x + offset_x, y + offset_y, z + offset_z));
                }
            }
        }
    }

    /// `createDataLayer(sectionNode)`.
    fn create_data_layer(&self, section_node: i64) -> Arc<DataLayer> {
        if let Some(queued) = self.queued_sections.lock().expect("queued light sections").get(&section_node) {
            return queued.clone();
        }
        match self.layer {
            LightLayer::Block => Arc::new(DataLayer::new()),
            LightLayer::Sky => {
                let top_section = self.updating.top_section(SectionPos::get_zero_node(section_node));
                if top_section != self.updating.current_lowest_y && SectionPos::y_from_long(section_node) < top_section {
                    let mut above_section = SectionPos::offset_long_direction(section_node, Direction::Up);
                    let above_data = loop {
                        if let Some(data) = self.updating.map.get(&above_section) {
                            break data;
                        }
                        above_section = SectionPos::offset_long_direction(above_section, Direction::Up);
                    };
                    Arc::new(repeat_first_layer(above_data))
                } else if self.light_on_in_section(section_node) {
                    Arc::new(DataLayer::filled(15))
                } else {
                    Arc::new(DataLayer::new())
                }
            }
        }
    }

    pub fn has_inconsistencies(&self) -> bool {
        self.has_inconsistencies
    }

    /// `markNewInconsistencies`.
    pub fn mark_new_inconsistencies(&mut self) {
        if !self.has_inconsistencies {
            return;
        }
        self.has_inconsistencies = false;
        let to_remove: Vec<i64> = self.to_remove.iter().copied().collect();
        {
            let mut queued_sections = self.queued_sections.lock().expect("queued light sections");
            for &node in &to_remove {
                let queued = queued_sections.remove(&node);
                let stored = self.updating.map.remove(&node);
                if self.columns_to_retain_queued_data_for.contains(&SectionPos::get_zero_node(node)) {
                    if let Some(queued) = queued {
                        queued_sections.insert(node, queued);
                    } else if let Some(stored) = stored {
                        queued_sections.insert(node, stored);
                    }
                }
            }
        }
        for &node in &to_remove {
            self.on_node_removed(node);
            self.changed_sections.insert(node);
        }
        self.to_remove.clear();
        let mut queued_sections = self.queued_sections.lock().expect("queued light sections");
        let nodes: Vec<i64> = queued_sections.keys().copied().collect();
        for section_node in nodes {
            if self.updating.map.contains_key(&section_node) {
                let data = queued_sections.remove(&section_node).expect("queued");
                let same = self.updating.map.get(&section_node).is_some_and(|l| Arc::ptr_eq(l, &data));
                if !same {
                    self.updating.map.insert(section_node, data);
                    self.changed_sections.insert(section_node);
                }
            }
        }
    }

    fn on_node_added(&mut self, section_node: i64) {
        if self.layer != LightLayer::Sky {
            return;
        }
        let y = SectionPos::y_from_long(section_node);
        if self.updating.current_lowest_y > y {
            self.updating.current_lowest_y = y;
        }
        let zero = SectionPos::get_zero_node(section_node);
        let old_top = self.updating.top_section(zero);
        if old_top < y + 1 {
            self.updating.top_sections.insert(zero, y + 1);
        }
    }

    fn on_node_removed(&mut self, section_node: i64) {
        if self.layer != LightLayer::Sky {
            return;
        }
        let zero = SectionPos::get_zero_node(section_node);
        let mut y = SectionPos::y_from_long(section_node);
        if self.updating.top_section(zero) == y + 1 {
            let mut new_top_section = section_node;
            while !self.storing_light_for_section(new_top_section) && self.has_light_data_at_or_below(y) {
                y -= 1;
                new_top_section = SectionPos::offset_long_direction(new_top_section, Direction::Down);
            }
            if self.storing_light_for_section(new_top_section) {
                self.updating.top_sections.insert(zero, y + 1);
            } else {
                self.updating.top_sections.remove(&zero);
            }
        }
    }

    pub fn set_light_enabled(&mut self, zero_node: i64, enable: bool) {
        if enable {
            self.columns_with_sources.insert(zero_node);
        } else {
            self.columns_with_sources.remove(&zero_node);
        }
    }

    pub fn light_on_in_section(&self, section_node: i64) -> bool {
        self.columns_with_sources.contains(&SectionPos::get_zero_node(section_node))
    }

    pub fn light_on_in_column(&self, section_zero_node: i64) -> bool {
        self.columns_with_sources.contains(&section_zero_node)
    }

    pub fn retain_data(&mut self, zero_node: i64, retain: bool) {
        if retain {
            self.columns_to_retain_queued_data_for.insert(zero_node);
        } else {
            self.columns_to_retain_queued_data_for.remove(&zero_node);
        }
    }

    pub fn queue_section_data(&mut self, section_node: i64, data: Option<DataLayer>) {
        let mut queued = self.queued_sections.lock().expect("queued light sections");
        match data {
            Some(data) => {
                queued.insert(section_node, Arc::new(data));
                self.has_inconsistencies = true;
            }
            None => {
                queued.remove(&section_node);
            }
        }
    }

    /// `updateSectionStatus(sectionNode, sectionEmpty)`.
    pub fn update_section_status(&mut self, section_node: i64, section_empty: bool) {
        let state = self.section_states.get(&section_node).copied().unwrap_or(0);
        let new_state = section_state::has_data_set(state, !section_empty);
        if state != new_state {
            self.put_section_state(section_node, new_state);
            let neighbor_increment = if section_empty { -1 } else { 1 };
            for offset_x in -1..=1 {
                for offset_y in -1..=1 {
                    for offset_z in -1..=1 {
                        if offset_x != 0 || offset_y != 0 || offset_z != 0 {
                            let neighbor_node = SectionPos::offset_long(section_node, offset_x, offset_y, offset_z);
                            let neighbor_state = self.section_states.get(&neighbor_node).copied().unwrap_or(0);
                            let count = section_state::neighbor_count(neighbor_state) + neighbor_increment;
                            self.put_section_state(neighbor_node, section_state::neighbor_count_set(neighbor_state, count));
                        }
                    }
                }
            }
        }
    }

    fn put_section_state(&mut self, section_node: i64, state: u8) {
        if state != 0 {
            if self.section_states.insert(section_node, state).unwrap_or(0) == 0 {
                self.initialize_section(section_node);
            }
        } else if self.section_states.remove(&section_node).unwrap_or(0) != 0 {
            self.remove_section(section_node);
        }
    }

    fn initialize_section(&mut self, section_node: i64) {
        if !self.to_remove.remove(&section_node) {
            let layer = self.create_data_layer(section_node);
            self.updating.map.insert(section_node, layer);
            self.changed_sections.insert(section_node);
            self.on_node_added(section_node);
            self.mark_section_and_neighbors_as_affected(section_node);
            self.has_inconsistencies = true;
        }
    }

    fn remove_section(&mut self, section_node: i64) {
        self.to_remove.insert(section_node);
        self.has_inconsistencies = true;
    }

    /// `swapSectionMap()`; returns the sections whose light changed (vanilla's `onLightUpdate` callbacks, in the
    /// iteration order of its `LongOpenHashSet`).
    pub fn swap_section_map(&mut self) -> Vec<i64> {
        if !self.changed_sections.is_empty() {
            let copy = Arc::new(self.updating.clone());
            *self.visible.write().expect("visible light map") = copy;
            self.changed_sections.clear();
        }
        let affected: Vec<i64> = self.sections_affected_by_light_updates.iter().collect();
        self.sections_affected_by_light_updates.clear();
        affected
    }

    pub fn get_debug_section_type(&self, section_node: i64) -> SectionType {
        match self.section_states.get(&section_node).copied().unwrap_or(0) {
            0 => SectionType::Empty,
            s if section_state::has_data(s) => SectionType::LightAndData,
            _ => SectionType::LightOnly,
        }
    }

    // ---- sky-light specific queries (SkyLightSectionStorage) ----

    pub fn has_light_data_at_or_below(&self, section_y: i32) -> bool {
        section_y >= self.updating.current_lowest_y
    }

    pub fn is_above_data(&self, section_node: i64) -> bool {
        let top_section = self.updating.top_section(SectionPos::get_zero_node(section_node));
        top_section == self.updating.current_lowest_y || SectionPos::y_from_long(section_node) >= top_section
    }

    pub fn get_top_section_y(&self, zero_node: i64) -> i32 {
        self.updating.top_section(zero_node)
    }

    pub fn get_bottom_section_y(&self) -> i32 {
        self.updating.current_lowest_y
    }
}

/// `SkyLightSectionStorage.repeatFirstLayer`: every horizontal slice becomes the lowest slice of `data`.
fn repeat_first_layer(data: &DataLayer) -> DataLayer {
    if data.is_definitely_homogenous() {
        return data.copy();
    }
    let input = data.data();
    let mut output = vec![0u8; 2048];
    for i in 0..16 {
        output[i * 128..(i + 1) * 128].copy_from_slice(&input[..128]);
    }
    DataLayer::from_bytes(output).expect("2048 bytes")
}
