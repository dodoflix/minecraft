//! `PoiManager` (+ the `SectionStorage` it extends): the points of interest of one dimension, stored per 16x16x16
//! section, persisted in `<dimension>/poi/r.X.Z.mca`.
//!
//! Vanilla's streams are lazy: `find`/`take`/`getRandom` stop loading POI chunks at the first hit, and a section is only
//! read from disk when a query first touches it. The queries here keep that: sections are visited in the same order and
//! a query stops at the same record. Results are `Vec`s / `Option`s in vanilla's stream order.
//!
//! The manager has interior mutability (`&self` API) because queries load sections on demand, exactly like vanilla's
//! `getPoiManager()` from a `ServerLevel`. A borrow is only held while a section is loaded/read; predicates and filters
//! run with the manager unborrowed, so they may use the level (and the manager) freely.

use super::poi_codec::{self, PackedPoiSection};
use super::poi_record::PoiRecord;
use super::poi_section::{Occupancy, PoiNeverRegistered, PoiSection};
use super::poi_type::{PoiTypeHolder, PoiTypes};
use crate::server::level::dynamic_graph::{GraphBehavior, GraphState};
use indexmap::IndexSet;
use mc_core::pos::{BlockPos, ChunkPos, SectionPos};
use mc_core::random::RandomSource;
use mc_data::misc::POI_TYPES;
use mc_nbt::CompoundTag;
use mc_world::chunk::LevelHeight;
use mc_world::level_storage::WORLD_VERSION;
use mc_world::region::{RegionStorageInfo, SimpleRegionStorage};
use mc_world::section::LevelChunkSection;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

pub const MAX_VILLAGE_DISTANCE: i32 = 6;
pub const VILLAGE_SECTION_SIZE: i32 = 1;

/// `PoiTypeTags.VILLAGE` of the vanilla data pack (`#acquirable_job_site`, `home`, `meeting`) as a bit set of type indices.
pub fn vanilla_village_types() -> u32 {
    use super::poi_type::PoiTypes as T;
    [
        T::ARMORER,
        T::BUTCHER,
        T::CARTOGRAPHER,
        T::CLERIC,
        T::FARMER,
        T::FISHERMAN,
        T::FLETCHER,
        T::LEATHERWORKER,
        T::LIBRARIAN,
        T::MASON,
        T::SHEPHERD,
        T::TOOLSMITH,
        T::WEAPONSMITH,
        T::HOME,
        T::MEETING,
    ]
    .iter()
    .filter_map(|name| PoiTypeHolder::by_name(name))
    .fold(0, |mask, t| mask | 1 << t.index())
}

/// Where the manager reads `PoiTypeTags.VILLAGE` from.
fn is_village_type(mask: u32, t: PoiTypeHolder) -> bool {
    mask >> t.index() & 1 != 0
}

/// `PoiManager.DistanceTracker` behaviour (`SectionTracker(7, 16, 256)`): the levels map plus read access to the sections.
struct SectionTrackerView<'a> {
    levels: &'a mut HashMap<i64, u8>,
    sections: &'a HashMap<i64, Option<PoiSection>>,
    village_types: u32,
}

impl SectionTrackerView<'_> {
    /// `getLevelFromSource(to)`: `isVillageCenter(to) ? 0 : 7`.
    fn level_from_source(&self, to: i64) -> i32 {
        match self.sections.get(&to) {
            Some(Some(section)) => {
                let mask = self.village_types;
                if section.has_record(&|t| is_village_type(mask, t), Occupancy::IsOccupied) {
                    0
                } else {
                    7
                }
            }
            _ => 7,
        }
    }
}

impl GraphBehavior for SectionTrackerView<'_> {
    fn is_source(&self, node: i64) -> bool {
        node == i64::MAX
    }

    fn get_computed_level(
        &mut self,
        _st: &GraphState,
        node: i64,
        known_parent: i64,
        known_level_from_parent: i32,
    ) -> i32 {
        let mut computed_level = known_level_from_parent;
        for offset_x in -1..=1 {
            for offset_y in -1..=1 {
                for offset_z in -1..=1 {
                    let mut neighbor = SectionPos::offset_long(node, offset_x, offset_y, offset_z);
                    if neighbor == node {
                        neighbor = i64::MAX;
                    }
                    if neighbor != known_parent {
                        let level = self.get_level(neighbor);
                        let cost_from_neighbor =
                            self.compute_level_from_neighbor(neighbor, node, level);
                        if computed_level > cost_from_neighbor {
                            computed_level = cost_from_neighbor;
                        }
                        if computed_level == 0 {
                            return computed_level;
                        }
                    }
                }
            }
        }
        computed_level
    }

    fn check_neighbors_after_update(
        &mut self,
        st: &mut GraphState,
        node: i64,
        level: i32,
        only_decrease: bool,
    ) {
        if !only_decrease || level < st.level_count - 2 {
            for offset_x in -1..=1 {
                for offset_y in -1..=1 {
                    for offset_z in -1..=1 {
                        let neighbor = SectionPos::offset_long(node, offset_x, offset_y, offset_z);
                        if neighbor != node {
                            st.check_neighbor(self, node, neighbor, level, only_decrease);
                        }
                    }
                }
            }
        }
    }

    fn get_level(&mut self, node: i64) -> i32 {
        i32::from(self.levels.get(&node).copied().unwrap_or(7))
    }

    fn set_level(&mut self, node: i64, level: i32) {
        if level > 6 {
            self.levels.remove(&node);
        } else {
            self.levels.insert(node, level as u8);
        }
    }

    fn compute_level_from_neighbor(&mut self, from: i64, to: i64, from_level: i32) -> i32 {
        if self.is_source(from) {
            self.level_from_source(to)
        } else {
            from_level + 1
        }
    }
}

/// `SectionStorage` + `PoiManager` state.
struct Storage {
    storage: SimpleRegionStorage,
    level_height: LevelHeight,
    /// `storage`: `None` (`Optional.empty`) = loaded chunk without a section at that y.
    sections: HashMap<i64, Option<PoiSection>>,
    /// `dirtyChunks` (`LongLinkedOpenHashSet`).
    dirty_chunks: IndexSet<i64>,
    /// `SectionStorage.loadedChunks`.
    loaded_chunks: HashSet<i64>,
    /// `PoiManager.distanceTracker`.
    tracker_state: GraphState,
    tracker_levels: HashMap<i64, u8>,
    /// `PoiManager.loadedChunks` (`ensureLoadedAndValid`).
    ensured_chunks: HashSet<i64>,
    village_types: u32,
}

impl Storage {
    fn outside_stored_range(&self, section_pos: i64) -> bool {
        let y = SectionPos::section_to_block_coord(SectionPos::y_from_long(section_pos));
        self.level_height.is_outside_build_height(y)
    }

    /// `getOrLoad(sectionPos)`.
    fn get_or_load(&mut self, section_pos: i64) -> Option<&mut PoiSection> {
        if self.outside_stored_range(section_pos) {
            return None;
        }
        if !self.sections.contains_key(&section_pos) {
            self.unpack_chunk(SectionPos::of_long(section_pos).chunk());
        }
        self.sections.get_mut(&section_pos).and_then(Option::as_mut)
    }

    /// `getOrCreate(sectionPos)`; `None` (logged) outside the build height, where vanilla throws.
    fn get_or_create(&mut self, section_pos: i64) -> Option<&mut PoiSection> {
        if self.outside_stored_range(section_pos) {
            log::error!(
                "Error executing task: sectionPos out of bounds ({:?})",
                SectionPos::of_long(section_pos)
            );
            return None;
        }
        if self.get_or_load(section_pos).is_none() {
            self.sections.insert(section_pos, Some(PoiSection::new()));
        }
        self.sections.get_mut(&section_pos).and_then(Option::as_mut)
    }

    /// `unpackChunk(chunkPos)`: read (blocking) and unpack the chunk's sections.
    fn unpack_chunk(&mut self, chunk_pos: ChunkPos) {
        if !self.loaded_chunks.insert(chunk_pos.pack()) {
            return;
        }
        let tag = match self.storage.read(&chunk_pos) {
            Ok(tag) => tag,
            Err(e) => {
                log::error!("Error reading chunk {chunk_pos} data from disk: {e}");
                None
            }
        };
        let packed = tag.map(|tag| {
            poi_codec::decode_chunk(
                &tag,
                self.level_height.min_section_y(),
                self.level_height.max_section_y(),
            )
        });
        self.unpack_packed(chunk_pos, packed);
    }

    /// `unpackChunk(pos, packedChunk)`. `versionChanged` is always `true` in vanilla (`upgradeChunkTag(Dynamic)` returns a
    /// new object, compared by reference), so every loaded section is marked dirty.
    fn unpack_packed(&mut self, chunk_pos: ChunkPos, packed: Option<Vec<PackedPoiSection>>) {
        let (min_y, max_y) = (
            self.level_height.min_section_y(),
            self.level_height.max_section_y(),
        );
        match packed {
            None => {
                for section_y in min_y..=max_y {
                    self.sections.insert(
                        SectionPos::as_long_xyz(chunk_pos.x, section_y, chunk_pos.z),
                        None,
                    );
                }
            }
            Some(packed) => {
                let mut by_y: HashMap<i32, (bool, Vec<PoiRecord>)> = packed.into_iter().collect();
                for section_y in min_y..=max_y {
                    let key = SectionPos::as_long_xyz(chunk_pos.x, section_y, chunk_pos.z);
                    let section = by_y
                        .remove(&section_y)
                        .map(|(valid, records)| PoiSection::unpack(valid, records));
                    let present = section.is_some();
                    self.sections.insert(key, section);
                    if present {
                        self.on_section_load(key);
                        self.set_dirty(key);
                    }
                }
            }
        }
    }

    /// `writeChunk(chunkPos)`.
    fn write_chunk(&mut self, chunk_pos: ChunkPos) {
        let mut tags = Vec::new();
        for section_y in self.level_height.min_section_y()..=self.level_height.max_section_y() {
            let key = SectionPos::as_long_xyz(chunk_pos.x, section_y, chunk_pos.z);
            if let Some(Some(section)) = self.sections.get(&key) {
                let (is_valid, records) = section.pack();
                tags.push((section_y, poi_codec::encode_section(is_valid, &records)));
            }
        }
        let tag: CompoundTag = poi_codec::encode_chunk(tags, WORLD_VERSION);
        self.storage.write(&chunk_pos, Some(tag));
        // vanilla's IOWorker thread stores the queued chunk in the background
        if let Err(e) = self.storage.synchronize(false) {
            log::error!("Failed to store POI chunk {chunk_pos}: {e}");
        }
    }

    /// `setDirty(sectionPos)` (`PoiManager`: also refreshes the village distance).
    fn set_dirty(&mut self, section_pos: i64) {
        if matches!(self.sections.get(&section_pos), Some(Some(_))) {
            self.dirty_chunks.insert(ChunkPos::pack_xz(
                SectionPos::x_from_long(section_pos),
                SectionPos::z_from_long(section_pos),
            ));
        } else {
            log::warn!(
                "No data for position: {:?}",
                SectionPos::of_long(section_pos)
            );
        }
        self.update_tracker(section_pos);
    }

    /// `distanceTracker.update(sectionPos, getLevelFromSource(sectionPos), false)`.
    fn update_tracker(&mut self, section_pos: i64) {
        let Self {
            tracker_state,
            tracker_levels,
            sections,
            village_types,
            ..
        } = self;
        let mut view = SectionTrackerView {
            levels: tracker_levels,
            sections,
            village_types: *village_types,
        };
        let level = view.level_from_source(section_pos);
        tracker_state.check_edge(&mut view, i64::MAX, section_pos, level, false);
    }

    fn on_section_load(&mut self, section_pos: i64) {
        self.update_tracker(section_pos);
    }

    /// `distanceTracker.runAllUpdates()`.
    fn run_all_updates(&mut self) {
        let Self {
            tracker_state,
            tracker_levels,
            sections,
            village_types,
            ..
        } = self;
        let mut view = SectionTrackerView {
            levels: tracker_levels,
            sections,
            village_types: *village_types,
        };
        tracker_state.run_updates(&mut view, i32::MAX);
    }

    fn tracker_level(&self, node: i64) -> i32 {
        i32::from(self.tracker_levels.get(&node).copied().unwrap_or(7))
    }

    /// Records of one section matching `mask` (types by index) and `occupancy`, in query order, with their section ids.
    fn section_records(
        &mut self,
        section_pos: i64,
        mask: &[bool],
        occupancy: Occupancy,
    ) -> Vec<(u32, PoiRecord)> {
        let mut out = Vec::new();
        if let Some(section) = self.get_or_load(section_pos) {
            section.for_each_record(&|t| mask[t.index()], occupancy, &mut |id, record| {
                out.push((id, *record));
                true
            });
        }
        out
    }
}

/// `ChunkPos.rangeClosed(from, to)`: x runs fastest from `from.x` towards `to.x`, z from `from.z` towards `to.z`.
fn range_closed(from: ChunkPos, to: ChunkPos) -> impl Iterator<Item = ChunkPos> {
    let x_size = (i64::from(from.x) - i64::from(to.x)).abs() + 1;
    let z_size = (i64::from(from.z) - i64::from(to.z)).abs() + 1;
    let x_diff: i32 = if from.x < to.x { 1 } else { -1 };
    let z_diff: i32 = if from.z < to.z { 1 } else { -1 };
    (0..z_size).flat_map(move |dz| {
        (0..x_size).map(move |dx| {
            ChunkPos::new(
                from.x.wrapping_add(dx as i32 * x_diff),
                from.z.wrapping_add(dz as i32 * z_diff),
            )
        })
    })
}

pub struct PoiManager {
    inner: RefCell<Storage>,
}

/// `Predicate<Holder<PoiType>>`.
pub type PoiTypePredicate<'a> = &'a dyn Fn(PoiTypeHolder) -> bool;

fn type_mask(predicate: PoiTypePredicate) -> Vec<bool> {
    (0..POI_TYPES.len())
        .map(|i| PoiTypeHolder::from_index(i).is_some_and(predicate))
        .collect()
}

impl PoiManager {
    /// `new PoiManager(info, folder, ...)`.
    pub fn new(
        info: RegionStorageInfo,
        folder: impl Into<PathBuf>,
        sync_writes: bool,
        level_height: LevelHeight,
    ) -> Self {
        Self {
            inner: RefCell::new(Storage {
                storage: SimpleRegionStorage::new(info, folder, sync_writes),
                level_height,
                sections: HashMap::new(),
                dirty_chunks: IndexSet::new(),
                loaded_chunks: HashSet::new(),
                tracker_state: GraphState::new(7),
                tracker_levels: HashMap::new(),
                ensured_chunks: HashSet::new(),
                village_types: vanilla_village_types(),
            }),
        }
    }

    /// Re-reads `PoiTypeTags.VILLAGE` (the tag of the loaded data packs) for later distance updates.
    pub fn set_village_types(&self, mask: u32) {
        self.inner.borrow_mut().village_types = mask;
    }

    // ------------------------------------------------------------------ modification

    /// `add(pos, type)`: the new record, `None` if the position already holds this type.
    pub fn add(&self, pos: BlockPos, poi_type: PoiTypeHolder) -> Option<PoiRecord> {
        let mut s = self.inner.borrow_mut();
        let key = SectionPos::as_long_block_pos(&pos);
        let record = s.get_or_create(key)?.add(pos, poi_type);
        if record.is_some() {
            s.set_dirty(key);
        }
        record
    }

    /// The deferred body of `ServerLevel.updatePOIOnBlockStateChange`: the removal task, then the registration task.
    pub fn apply_block_state_change(
        &self,
        pos: BlockPos,
        old_type: Option<PoiTypeHolder>,
        new_type: Option<PoiTypeHolder>,
    ) {
        if old_type.is_some() {
            self.remove(pos);
        }
        if let Some(poi_type) = new_type {
            self.add(pos, poi_type);
        }
    }

    /// `remove(pos)`.
    pub fn remove(&self, pos: BlockPos) {
        let mut s = self.inner.borrow_mut();
        let key = SectionPos::as_long_block_pos(&pos);
        if s.get_or_load(key)
            .is_some_and(|section| section.remove(pos))
        {
            s.set_dirty(key);
        }
    }

    /// `release(pos)`: `Err` is vanilla's `IllegalStateException("POI never registered at ...")`.
    pub fn release(&self, pos: BlockPos) -> Result<bool, PoiNeverRegistered> {
        let mut s = self.inner.borrow_mut();
        let key = SectionPos::as_long_block_pos(&pos);
        let success = s
            .get_or_load(key)
            .ok_or(PoiNeverRegistered(pos))?
            .release(pos)?;
        s.set_dirty(key);
        Ok(success)
    }

    /// `PoiManager.checkConsistencyWithBlocks(sectionPos, blockSection)`.
    pub fn check_consistency_with_blocks(
        &self,
        section_pos: SectionPos,
        block_section: &LevelChunkSection,
    ) {
        let mut s = self.inner.borrow_mut();
        let key = section_pos.as_long();
        let may_have_poi = block_section.maybe_has(PoiTypes::has_poi);
        let mut changed = false;
        if let Some(section) = s.get_or_load(key) {
            changed = section.refresh(|output| {
                if may_have_poi {
                    update_from_section(block_section, section_pos, output);
                }
            });
        } else if may_have_poi && let Some(new_section) = s.get_or_create(key) {
            update_from_section(block_section, section_pos, &mut |pos, poi_type| {
                changed |= new_section.add(pos, poi_type).is_some();
            });
        }
        if changed {
            s.set_dirty(key);
        }
    }

    // ------------------------------------------------------------------ queries (stream order)

    /// Visits the records of one chunk (`getInChunk`): sections bottom to top, each loaded when reached. `f` returns
    /// `false` to stop; the result says whether the walk ran to the end.
    fn walk_chunk(
        &self,
        mask: &[bool],
        chunk_pos: ChunkPos,
        occupancy: Occupancy,
        f: &mut dyn FnMut(i64, u32, &PoiRecord) -> bool,
    ) -> bool {
        let (min_y, max_y) = {
            let s = self.inner.borrow();
            (
                s.level_height.min_section_y(),
                s.level_height.max_section_y(),
            )
        };
        for section_y in min_y..=max_y {
            let key = SectionPos::of_chunk(&chunk_pos, section_y).as_long();
            let hits = self
                .inner
                .borrow_mut()
                .section_records(key, mask, occupancy);
            for (id, record) in &hits {
                if !f(key, *id, record) {
                    return false;
                }
            }
        }
        true
    }

    /// `getInSquare` walk.
    fn walk_square(
        &self,
        mask: &[bool],
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
        f: &mut dyn FnMut(i64, u32, &PoiRecord) -> bool,
    ) {
        let chunk_radius = radius.div_euclid(16).wrapping_add(1);
        let c = ChunkPos::containing(&center);
        let from = ChunkPos::new(
            c.x.wrapping_sub(chunk_radius),
            c.z.wrapping_sub(chunk_radius),
        );
        let to = ChunkPos::new(
            c.x.wrapping_add(chunk_radius),
            c.z.wrapping_add(chunk_radius),
        );
        for chunk_pos in range_closed(from, to) {
            let keep_going = self.walk_chunk(mask, chunk_pos, occupancy, &mut |key, id, record| {
                let pos = record.get_pos();
                if pos.x().wrapping_sub(center.x()).wrapping_abs() <= radius
                    && pos.z().wrapping_sub(center.z()).wrapping_abs() <= radius
                {
                    f(key, id, record)
                } else {
                    true
                }
            });
            if !keep_going {
                return;
            }
        }
    }

    /// `getInRange` walk.
    fn walk_range(
        &self,
        mask: &[bool],
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
        f: &mut dyn FnMut(i64, u32, &PoiRecord) -> bool,
    ) {
        let radius_sqr = radius.wrapping_mul(radius);
        self.walk_square(mask, center, radius, occupancy, &mut |key, id, record| {
            if record.get_pos().dist_sqr(&center) <= f64::from(radius_sqr) {
                f(key, id, record)
            } else {
                true
            }
        });
    }

    /// `getCountInRange(predicate, center, radius, occupancy)`.
    pub fn get_count_in_range(
        &self,
        predicate: PoiTypePredicate,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
    ) -> i64 {
        let mut count = 0;
        self.walk_range(
            &type_mask(predicate),
            center,
            radius,
            occupancy,
            &mut |_, _, _| {
                count += 1;
                true
            },
        );
        count
    }

    /// `existsAtPosition(poiType, blockPos)`.
    pub fn exists_at_position(&self, poi_type: &str, pos: BlockPos) -> bool {
        self.exists(pos, &|t| t.is(poi_type))
    }

    /// `getInSquare(predicate, center, radius, occupancy)`.
    pub fn get_in_square(
        &self,
        predicate: PoiTypePredicate,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
    ) -> Vec<PoiRecord> {
        let mut out = Vec::new();
        self.walk_square(
            &type_mask(predicate),
            center,
            radius,
            occupancy,
            &mut |_, _, record| {
                out.push(*record);
                true
            },
        );
        out
    }

    /// `getInRange(predicate, center, radius, occupancy)`.
    pub fn get_in_range(
        &self,
        predicate: PoiTypePredicate,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
    ) -> Vec<PoiRecord> {
        let mut out = Vec::new();
        self.walk_range(
            &type_mask(predicate),
            center,
            radius,
            occupancy,
            &mut |_, _, record| {
                out.push(*record);
                true
            },
        );
        out
    }

    /// `getInChunk(predicate, chunkPos, occupancy)`.
    pub fn get_in_chunk(
        &self,
        predicate: PoiTypePredicate,
        chunk_pos: ChunkPos,
        occupancy: Occupancy,
    ) -> Vec<PoiRecord> {
        let mut out = Vec::new();
        self.walk_chunk(
            &type_mask(predicate),
            chunk_pos,
            occupancy,
            &mut |_, _, record| {
                out.push(*record);
                true
            },
        );
        out
    }

    /// `findAll(predicate, filter, center, radius, occupancy)`.
    pub fn find_all(
        &self,
        predicate: PoiTypePredicate,
        filter: &dyn Fn(BlockPos) -> bool,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
    ) -> Vec<BlockPos> {
        let mut out = Vec::new();
        self.walk_range(
            &type_mask(predicate),
            center,
            radius,
            occupancy,
            &mut |_, _, record| {
                if filter(record.get_pos()) {
                    out.push(record.get_pos());
                }
                true
            },
        );
        out
    }

    /// `findAllWithType(predicate, filter, center, radius, occupancy)`.
    pub fn find_all_with_type(
        &self,
        predicate: PoiTypePredicate,
        filter: &dyn Fn(BlockPos) -> bool,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
    ) -> Vec<(PoiTypeHolder, BlockPos)> {
        let mut out = Vec::new();
        self.walk_range(
            &type_mask(predicate),
            center,
            radius,
            occupancy,
            &mut |_, _, record| {
                if filter(record.get_pos()) {
                    out.push((record.get_poi_type(), record.get_pos()));
                }
                true
            },
        );
        out
    }

    /// `findAllClosestFirstWithType(...)`: `findAllWithType` stably sorted by `distSqr` to `center`.
    pub fn find_all_closest_first_with_type(
        &self,
        predicate: PoiTypePredicate,
        filter: &dyn Fn(BlockPos) -> bool,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
    ) -> Vec<(PoiTypeHolder, BlockPos)> {
        let mut out = self.find_all_with_type(predicate, filter, center, radius, occupancy);
        out.sort_by(|a, b| a.1.dist_sqr(&center).total_cmp(&b.1.dist_sqr(&center)));
        out
    }

    /// `find(predicate, filter, center, radius, occupancy)`: the first match (loads sections only up to it).
    pub fn find(
        &self,
        predicate: PoiTypePredicate,
        filter: &dyn Fn(BlockPos) -> bool,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
    ) -> Option<BlockPos> {
        let mut found = None;
        self.walk_range(
            &type_mask(predicate),
            center,
            radius,
            occupancy,
            &mut |_, _, record| {
                if filter(record.get_pos()) {
                    found = Some(record.get_pos());
                    false
                } else {
                    true
                }
            },
        );
        found
    }

    /// `min(Comparator.comparingDouble(distSqr))` over the range: the first of equally close records wins.
    fn closest(
        &self,
        predicate: PoiTypePredicate,
        filter: &dyn Fn(&PoiRecord) -> bool,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
    ) -> Option<PoiRecord> {
        let mut best: Option<(f64, PoiRecord)> = None;
        self.walk_range(
            &type_mask(predicate),
            center,
            radius,
            occupancy,
            &mut |_, _, record| {
                if filter(record) {
                    let dist = record.get_pos().dist_sqr(&center);
                    if best.is_none_or(|(d, _)| d.total_cmp(&dist).is_gt()) {
                        best = Some((dist, *record));
                    }
                }
                true
            },
        );
        best.map(|(_, record)| record)
    }

    /// `findClosest(predicate, center, radius, occupancy)`.
    pub fn find_closest(
        &self,
        predicate: PoiTypePredicate,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
    ) -> Option<BlockPos> {
        self.closest(predicate, &|_| true, center, radius, occupancy)
            .map(|r| r.get_pos())
    }

    /// `findClosestWithType(predicate, center, radius, occupancy)`.
    pub fn find_closest_with_type(
        &self,
        predicate: PoiTypePredicate,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
    ) -> Option<(PoiTypeHolder, BlockPos)> {
        self.closest(predicate, &|_| true, center, radius, occupancy)
            .map(|r| (r.get_poi_type(), r.get_pos()))
    }

    /// `findClosest(predicate, filter, center, radius, occupancy)`.
    pub fn find_closest_filtered(
        &self,
        predicate: PoiTypePredicate,
        filter: &dyn Fn(BlockPos) -> bool,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
    ) -> Option<BlockPos> {
        self.closest(
            predicate,
            &|r| filter(r.get_pos()),
            center,
            radius,
            occupancy,
        )
        .map(|r| r.get_pos())
    }

    /// `take(predicate, filter, center, radius)`: acquires a ticket of the first `HAS_SPACE` record the filter accepts.
    pub fn take(
        &self,
        predicate: PoiTypePredicate,
        filter: &dyn Fn(PoiTypeHolder, BlockPos) -> bool,
        center: BlockPos,
        radius: i32,
    ) -> Option<BlockPos> {
        let mut found = None;
        self.walk_range(
            &type_mask(predicate),
            center,
            radius,
            Occupancy::HasSpace,
            &mut |key, id, record| {
                if filter(record.get_poi_type(), record.get_pos()) {
                    found = Some((key, id, record.get_pos()));
                    false
                } else {
                    true
                }
            },
        );
        let (key, id, pos) = found?;
        let mut s = self.inner.borrow_mut();
        if s.get_or_load(key)
            .is_some_and(|section| section.acquire_ticket(id))
        {
            s.set_dirty(key);
        }
        Some(pos)
    }

    /// `getRandom(predicate, filter, occupancy, center, radius, random)`: shuffles the whole range (`Util.toShuffledList`).
    pub fn get_random(
        &self,
        predicate: PoiTypePredicate,
        filter: &dyn Fn(BlockPos) -> bool,
        occupancy: Occupancy,
        center: BlockPos,
        radius: i32,
        random: &mut dyn RandomSource,
    ) -> Option<BlockPos> {
        let mut list = self.get_in_range(predicate, center, radius, occupancy);
        for i in (2..=list.len()).rev() {
            let swap_to = random.next_int_bounded(i as i32) as usize;
            list.swap(i - 1, swap_to);
        }
        list.iter()
            .find(|r| filter(r.get_pos()))
            .map(|r| r.get_pos())
    }

    /// `exists(pos, predicate)`.
    pub fn exists(&self, pos: BlockPos, predicate: PoiTypePredicate) -> bool {
        let t = self.get_type(pos);
        t.is_some_and(predicate)
    }

    /// `getType(pos)`.
    pub fn get_type(&self, pos: BlockPos) -> Option<PoiTypeHolder> {
        self.inner
            .borrow_mut()
            .get_or_load(SectionPos::as_long_block_pos(&pos))
            .and_then(|section| section.get_type(pos))
    }

    /// `getDebugPoiInfo(pos)`: the record at `pos` (`DebugPoiInfo` is built from it).
    pub fn get_debug_poi_info(&self, pos: BlockPos) -> Option<PoiRecord> {
        self.inner
            .borrow_mut()
            .get_or_load(SectionPos::as_long_block_pos(&pos))
            .and_then(|section| section.get_poi_record(pos))
    }

    /// `sectionsToVillage(sectionPos)`.
    pub fn sections_to_village(&self, section_pos: SectionPos) -> i32 {
        let mut s = self.inner.borrow_mut();
        s.run_all_updates();
        s.tracker_level(section_pos.as_long())
    }

    // ------------------------------------------------------------------ ensureLoadedAndValid

    /// The sections `ensureLoadedAndValid` visits: `SectionPos.aroundChunk(chunk(center), floorDiv(radius, 16), min, max)`.
    pub fn sections_around(
        &self,
        center: BlockPos,
        radius: i32,
    ) -> impl Iterator<Item = SectionPos> + use<> {
        let height = self.inner.borrow().level_height;
        SectionPos::around_chunk(
            &ChunkPos::containing(&center),
            radius.div_euclid(16),
            height.min_section_y(),
            height.max_section_y(),
        )
    }

    /// `getOrLoad(section).map(isValid).orElse(false)`.
    pub fn is_section_valid(&self, section_pos: SectionPos) -> bool {
        self.inner
            .borrow_mut()
            .get_or_load(section_pos.as_long())
            .is_some_and(|s| s.is_valid())
    }

    /// `loadedChunks.add(chunk.pack())` of `ensureLoadedAndValid`: whether the chunk was not requested yet.
    pub fn mark_chunk_ensured(&self, chunk_pos: ChunkPos) -> bool {
        self.inner
            .borrow_mut()
            .ensured_chunks
            .insert(chunk_pos.pack())
    }

    // ------------------------------------------------------------------ persistence / ticking

    /// `tick(haveTime)`: writes dirty chunks while there is time, then settles the village distances.
    pub fn tick(&self, have_time: &dyn Fn() -> bool) {
        let mut s = self.inner.borrow_mut();
        while !s.dirty_chunks.is_empty() && have_time() {
            let Some(key) = s.dirty_chunks.shift_remove_index(0) else {
                break;
            };
            s.write_chunk(ChunkPos::unpack(key));
        }
        s.run_all_updates();
    }

    /// `flush(chunkPos)`: writes the chunk now if it is dirty.
    pub fn flush(&self, chunk_pos: ChunkPos) {
        let mut s = self.inner.borrow_mut();
        if s.dirty_chunks.shift_remove(&chunk_pos.pack()) {
            s.write_chunk(chunk_pos);
        }
    }

    /// `flushAll()`.
    pub fn flush_all(&self) {
        let mut s = self.inner.borrow_mut();
        if !s.dirty_chunks.is_empty() {
            let dirty: Vec<i64> = s.dirty_chunks.drain(..).collect();
            for key in dirty {
                s.write_chunk(ChunkPos::unpack(key));
            }
        }
    }

    /// `hasWork()`.
    pub fn has_work(&self) -> bool {
        !self.inner.borrow().dirty_chunks.is_empty()
    }

    /// `close()`: stores everything queued and closes the region files.
    pub fn close(&self) {
        if let Err(e) = self.inner.borrow_mut().storage.close() {
            log::error!("Failed to close POI storage: {e}");
        }
    }
}

/// `updateFromSection`: every block of the section that is a point of interest, in `blocksInside` order.
fn update_from_section(
    block_section: &LevelChunkSection,
    pos: SectionPos,
    output: &mut dyn FnMut(BlockPos, PoiTypeHolder),
) {
    for block_pos in pos.blocks_inside() {
        let state = block_section.get_block_state(
            SectionPos::section_relative(block_pos.x()),
            SectionPos::section_relative(block_pos.y()),
            SectionPos::section_relative(block_pos.z()),
        );
        if let Some(poi_type) = PoiTypes::for_state(state) {
            output(block_pos, poi_type);
        }
    }
}
