//! `PoiSection`: the points of interest of one 16x16x16 section.
//!
//! Vanilla keeps every `PoiRecord` object in a `Short2ObjectOpenHashMap` (`records`, serialization order) *and* in a
//! `HashMap<Holder<PoiType>, HashSet<PoiRecord>>` (`byType`, the order every query sees). The record objects are shared
//! and mutable and the two containers can disagree (a position re-registered with another type leaves the old object in
//! its type's set), so the records live in a slab referenced from both. Observable orders:
//! * `pack()` (file content): fastutil *spliterator* order of `records.values()`: the `0` key, then the table ascending;
//! * `getRecords` (queries): types in `HashMap<Holder<PoiType>, _>` order (registry-key hash like the oracle's holders,
//!   see [`PoiTypeHolder::hash_code`]), each type's `HashSet<PoiRecord>` in Java order (`BlockPos.hashCode`, bucket order,
//!   insertion order inside a bucket, splitting at 0.75 load).
//!
//! shortcut: tree bins are not emulated (Java treeifies a bin of >= 9 colliding positions in a table of >= 64 buckets).

use super::poi_record::PoiRecord;
use super::poi_type::PoiTypeHolder;
use crate::server::level::fastutil::LongOpenHashMap;
use crate::util::java_hash_order::JavaHashCapacity;
use mc_core::pos::{BlockPos, SectionPos};
use mc_data::misc::POI_TYPES;

/// `PoiManager.Occupancy`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Occupancy {
    HasSpace,
    IsOccupied,
    Any,
}

impl Occupancy {
    /// `getTest().test(record)`.
    pub fn test(self, record: &PoiRecord) -> bool {
        match self {
            Occupancy::HasSpace => record.has_space(),
            Occupancy::IsOccupied => record.is_occupied(),
            Occupancy::Any => true,
        }
    }
}

/// `release` on a position without a record: vanilla's `IllegalStateException("POI never registered at ...")`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoiNeverRegistered(pub BlockPos);

impl std::fmt::Display for PoiNeverRegistered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "POI never registered at {}", self.0)
    }
}

impl std::error::Error for PoiNeverRegistered {}

#[derive(Clone, Copy)]
struct SetEntry {
    /// `HashMap.hash(record)`.
    hash: u32,
    /// `SectionPos.sectionRelativePos(pos)`: equal records of one section are equal positions.
    key: i16,
    id: u32,
}

/// `java.util.HashSet<PoiRecord>` (equality = position) in Java iteration order.
#[derive(Default)]
struct RecordSet {
    table: Vec<Vec<SetEntry>>,
    size: usize,
    threshold: usize,
}

impl RecordSet {
    fn add(&mut self, entry: SetEntry) -> bool {
        if self.table.is_empty() {
            self.resize();
        }
        let index = entry.hash as usize & (self.table.len() - 1);
        let bin = &mut self.table[index];
        if bin.iter().any(|e| e.key == entry.key) {
            return false;
        }
        bin.push(entry);
        // `treeifyBin` resizes a table below 64 buckets instead of treeifying
        if bin.len() >= 9 && self.table.len() < 64 {
            self.resize();
        }
        self.size += 1;
        if self.size > self.threshold {
            self.resize();
        }
        true
    }

    /// `remove(record)`: the element equal to the record (same position), if any.
    fn remove(&mut self, key: i16, hash: u32) -> Option<u32> {
        if self.table.is_empty() {
            return None;
        }
        let index = hash as usize & (self.table.len() - 1);
        let bin = &mut self.table[index];
        let at = bin.iter().position(|e| e.key == key)?;
        self.size -= 1;
        Some(bin.remove(at).id)
    }

    fn resize(&mut self) {
        let old_cap = self.table.len();
        let new_cap = if old_cap == 0 { 16 } else { old_cap * 2 };
        let mut table: Vec<Vec<SetEntry>> = vec![Vec::new(); new_cap];
        for (j, bin) in std::mem::take(&mut self.table).into_iter().enumerate() {
            for e in bin {
                table[if e.hash as usize & old_cap == 0 {
                    j
                } else {
                    j + old_cap
                }]
                .push(e);
            }
        }
        self.table = table;
        self.threshold = new_cap / 4 * 3;
    }

    fn iter(&self) -> impl Iterator<Item = &SetEntry> {
        self.table.iter().flatten()
    }
}

struct Slot {
    record: PoiRecord,
    /// Containers (`records` map, type sets) that reference the record object.
    refs: u8,
}

pub struct PoiSection {
    slab: Vec<Option<Slot>>,
    free: Vec<u32>,
    /// `records`: `sectionRelativePos` (as int) -> slab id; `Short2ObjectOpenHashMap` table layout.
    records: LongOpenHashMap<u32>,
    /// `byType`, indexed by the type's registration index.
    by_type: Vec<RecordSet>,
    /// The registration indexes of the keys of `byType` in insertion order (a key stays when its set empties).
    type_keys: Vec<usize>,
    /// Table length of the `byType` map (kept by `clear()`).
    type_capacity: JavaHashCapacity,
    is_valid: bool,
}

fn relative_key(pos: &BlockPos) -> i16 {
    SectionPos::section_relative_pos(pos)
}

/// `HashMap.hash(pos.hashCode())`.
fn spread_hash(pos: &BlockPos) -> u32 {
    let h = pos.hash_code() as u32;
    h ^ (h >> 16)
}

impl Default for PoiSection {
    fn default() -> Self {
        Self::new()
    }
}

impl PoiSection {
    /// `new PoiSection(setDirty)`: empty and valid.
    pub fn new() -> Self {
        Self::with_valid(true)
    }

    fn with_valid(is_valid: bool) -> Self {
        Self {
            slab: Vec::new(),
            free: Vec::new(),
            records: LongOpenHashMap::new_int(),
            by_type: (0..POI_TYPES.len()).map(|_| RecordSet::default()).collect(),
            type_keys: Vec::new(),
            type_capacity: JavaHashCapacity::default(),
            is_valid,
        }
    }

    /// `PoiSection.Packed.unpack`: `records.forEach(this::add)`.
    pub fn unpack(is_valid: bool, records: Vec<PoiRecord>) -> Self {
        let mut section = Self::with_valid(is_valid);
        for record in records {
            section.add_record(record);
        }
        section
    }

    /// `pack()`: `(isValid, records)` in `records.values()` stream order.
    pub fn pack(&self) -> (bool, Vec<PoiRecord>) {
        let mut out = Vec::with_capacity(self.records.len());
        self.records
            .for_each_stream_order(|_, &id| out.push(self.slot(id).record));
        (self.is_valid, out)
    }

    fn slot(&self, id: u32) -> &Slot {
        self.slab[id as usize].as_ref().expect("live record")
    }

    fn alloc(&mut self, record: PoiRecord) -> u32 {
        let slot = Some(Slot { record, refs: 0 });
        match self.free.pop() {
            Some(id) => {
                self.slab[id as usize] = slot;
                id
            }
            None => {
                self.slab.push(slot);
                (self.slab.len() - 1) as u32
            }
        }
    }

    fn inc_ref(&mut self, id: u32) {
        self.slab[id as usize].as_mut().expect("live record").refs += 1;
    }

    fn dec_ref(&mut self, id: u32) {
        let slot = self.slab[id as usize].as_mut().expect("live record");
        slot.refs -= 1;
        if slot.refs == 0 {
            self.slab[id as usize] = None;
            self.free.push(id);
        }
    }

    /// `getRecords(predicate, occupancy)`: calls `f(id, record)` in query order until it returns `false`.
    pub(super) fn for_each_record(
        &self,
        predicate: &dyn Fn(PoiTypeHolder) -> bool,
        occupancy: Occupancy,
        f: &mut dyn FnMut(u32, &PoiRecord) -> bool,
    ) {
        // `HashMap` order: bucket by the key hash, and `computeIfAbsent` links a new key at the head of its bin
        let order =
            self.type_capacity
                .iteration_order(self.type_keys.iter().rev().copied(), |&index| {
                    PoiTypeHolder::from_index(index)
                        .expect("registered type")
                        .hash_code()
                });
        for index in order {
            let set = &self.by_type[index];
            let holder = PoiTypeHolder::from_index(index).expect("registered type");
            if set.size == 0 || !predicate(holder) {
                continue;
            }
            for entry in set.iter() {
                let record = &self.slot(entry.id).record;
                if occupancy.test(record) && !f(entry.id, record) {
                    return;
                }
            }
        }
    }

    /// `getRecords(predicate, occupancy).findAny().isPresent()`.
    pub fn has_record(
        &self,
        predicate: &dyn Fn(PoiTypeHolder) -> bool,
        occupancy: Occupancy,
    ) -> bool {
        let mut found = false;
        self.for_each_record(predicate, occupancy, &mut |_, _| {
            found = true;
            false
        });
        found
    }

    /// `acquireTicket()` of the record `id` (the manager marks the section dirty on success).
    pub(super) fn acquire_ticket(&mut self, id: u32) -> bool {
        self.slab[id as usize]
            .as_mut()
            .expect("live record")
            .record
            .acquire_ticket()
    }

    /// Private `add(record)`: `false` when the position already holds a record of the same type.
    fn add_record(&mut self, record: PoiRecord) -> bool {
        let pos = record.get_pos();
        let poi_type = record.get_poi_type();
        let key = relative_key(&pos);
        if let Some(&old_id) = self.records.get(i64::from(key)) {
            if self.slot(old_id).record.get_poi_type() == poi_type {
                return false;
            }
            log::error!("POI data mismatch: already registered at {pos}");
        }
        let id = self.alloc(record);
        self.inc_ref(id);
        if let Some(old_id) = self.records.insert(i64::from(key), id) {
            self.dec_ref(old_id);
        }
        self.compute_type_set_if_absent(poi_type);
        if self.by_type[poi_type.index()].add(SetEntry {
            hash: spread_hash(&pos),
            key,
            id,
        }) {
            self.inc_ref(id);
        }
        true
    }

    /// `byType.computeIfAbsent(type, ...)`: a new key resizes the table first when the old size is past the threshold.
    fn compute_type_set_if_absent(&mut self, poi_type: PoiTypeHolder) {
        if !self.type_keys.contains(&poi_type.index()) {
            self.type_capacity.grow_to_fit(self.type_keys.len());
            self.type_keys.push(poi_type.index());
        }
    }

    /// Public `add(pos, type)`: the new record, `None` if the position already holds this type. The caller marks the
    /// section dirty when a record is returned.
    pub fn add(&mut self, pos: BlockPos, poi_type: PoiTypeHolder) -> Option<PoiRecord> {
        let record = PoiRecord::new(pos, poi_type);
        if self.add_record(record) {
            log::debug!("Added POI of type {} @ {}", poi_type.registered_name(), pos);
            Some(record)
        } else {
            None
        }
    }

    /// `remove(pos)`: whether a record was removed (the caller marks the section dirty then).
    pub fn remove(&mut self, pos: BlockPos) -> bool {
        let key = relative_key(&pos);
        let Some(id) = self.records.remove(i64::from(key)) else {
            log::error!("POI data mismatch: never registered at {pos}");
            return false;
        };
        let poi_type = self.slot(id).record.get_poi_type();
        if let Some(set_id) = self.by_type[poi_type.index()].remove(key, spread_hash(&pos)) {
            self.dec_ref(set_id);
        }
        self.dec_ref(id);
        log::debug!(
            "Removed POI of type {} @ {}",
            poi_type.registered_name(),
            pos
        );
        true
    }

    /// `getFreeTickets(pos)`.
    pub fn get_free_tickets(&self, pos: BlockPos) -> i32 {
        self.get_poi_record(pos).map_or(0, |r| r.get_free_tickets())
    }

    /// `release(pos)`: whether a ticket was released. The caller marks the section dirty either way.
    pub fn release(&mut self, pos: BlockPos) -> Result<bool, PoiNeverRegistered> {
        let Some(&id) = self.records.get(i64::from(relative_key(&pos))) else {
            return Err(PoiNeverRegistered(pos));
        };
        Ok(self.slab[id as usize]
            .as_mut()
            .expect("live record")
            .record
            .release_ticket())
    }

    /// `exists(pos, predicate)`.
    pub fn exists(&self, pos: BlockPos, predicate: &dyn Fn(PoiTypeHolder) -> bool) -> bool {
        self.get_type(pos).is_some_and(predicate)
    }

    /// `getType(pos)`.
    pub fn get_type(&self, pos: BlockPos) -> Option<PoiTypeHolder> {
        self.get_poi_record(pos).map(|r| r.get_poi_type())
    }

    /// `getPoiRecord(pos)` (`getDebugPoiInfo` is built from it).
    pub fn get_poi_record(&self, pos: BlockPos) -> Option<PoiRecord> {
        self.records
            .get(i64::from(relative_key(&pos)))
            .map(|&id| self.slot(id).record)
    }

    /// `refresh(updater)`: rebuilds an invalid section from the blocks (`updater` reports every block that is a point
    /// of interest). An old record at the same position is kept, whatever its type. The caller marks the section dirty
    /// when this returns `true`.
    pub fn refresh(
        &mut self,
        updater: impl FnOnce(&mut dyn FnMut(BlockPos, PoiTypeHolder)),
    ) -> bool {
        if self.is_valid {
            return false;
        }
        // `oldRecords = new Short2ObjectOpenHashMap<>(records)`; `computeIfAbsent` also adds the records it creates
        let mut old_records: Vec<(i16, u32)> = Vec::new();
        self.records
            .for_each_stream_order(|key, &id| old_records.push((key as i16, id)));
        for &(_, id) in &old_records {
            self.inc_ref(id);
        }
        self.clear();
        let mut created: Vec<(i16, u32)> = Vec::new();
        updater(&mut |pos, poi_type| {
            let key = relative_key(&pos);
            let id = match old_records
                .iter()
                .chain(created.iter())
                .find(|(k, _)| *k == key)
            {
                Some(&(_, id)) => id,
                None => {
                    let id = self.alloc(PoiRecord::new(pos, poi_type));
                    self.inc_ref(id);
                    created.push((key, id));
                    id
                }
            };
            self.add_existing(id);
        });
        for (_, id) in old_records.into_iter().chain(created) {
            self.dec_ref(id);
        }
        self.is_valid = true;
        true
    }

    /// `add(record)` for a record object that is already allocated.
    fn add_existing(&mut self, id: u32) {
        let record = self.slot(id).record;
        let pos = record.get_pos();
        let poi_type = record.get_poi_type();
        let key = relative_key(&pos);
        if let Some(&old_id) = self.records.get(i64::from(key)) {
            if self.slot(old_id).record.get_poi_type() == poi_type {
                return;
            }
            log::error!("POI data mismatch: already registered at {pos}");
        }
        self.inc_ref(id);
        if let Some(old_id) = self.records.insert(i64::from(key), id) {
            self.dec_ref(old_id);
        }
        self.compute_type_set_if_absent(poi_type);
        if self.by_type[poi_type.index()].add(SetEntry {
            hash: spread_hash(&pos),
            key,
            id,
        }) {
            self.inc_ref(id);
        }
    }

    /// `clear()`: `records.clear()` (table size kept) and `byType.clear()`.
    fn clear(&mut self) {
        let mut ids: Vec<u32> = Vec::new();
        self.records.for_each_stream_order(|_, &id| ids.push(id));
        self.records.clear();
        for set in &mut self.by_type {
            for bin in std::mem::take(&mut set.table) {
                ids.extend(bin.into_iter().map(|e| e.id));
            }
            *set = RecordSet::default();
        }
        self.type_keys.clear();
        for id in ids {
            self.dec_ref(id);
        }
    }

    pub fn is_valid(&self) -> bool {
        self.is_valid
    }
}
