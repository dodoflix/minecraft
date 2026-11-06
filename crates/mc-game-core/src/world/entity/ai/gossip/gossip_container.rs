//! `GossipContainer`: what a villager heard about other entities, per target and `GossipType`.
//!
//! The per-target map is a Java `HashMap<UUID, _>` whose iteration order is observable (transfer selection ranges,
//! the saved list order); it is emulated exactly: bucket index of `UUID.hashCode()` in a table that doubles
//! on `computeIfAbsent` (resize *before* inserting when `size > threshold`) and never shrinks. The entries of one target
//! (`Object2IntOpenHashMap<GossipType>`, enum identity hashes: JVM-random in vanilla) iterate in ordinal order.

use super::gossip_type::GossipType;
use mc_core::random::RandomSource;
use mc_core::uuid_util::{java_hash_code, uuid_from_int_array, uuid_to_int_array};
use mc_nbt::{CompoundTag, ListTag, Tag};
use uuid::Uuid;

/// `GossipContainer.DISCARD_THRESHOLD`.
pub const DISCARD_THRESHOLD: i32 = 2;

#[derive(Clone, Default, Debug, PartialEq)]
struct EntityGossips {
    /// Indexed by `GossipType::ordinal`.
    entries: [Option<i32>; 5],
}

impl EntityGossips {
    fn get_int(&self, t: GossipType) -> i32 {
        self.entries[t.ordinal()].unwrap_or(0)
    }

    fn weighted_value(&self, types: &impl Fn(GossipType) -> bool) -> i32 {
        GossipType::ALL
            .iter()
            .filter(|&&t| types(t))
            .filter_map(|&t| self.entries[t.ordinal()].map(|v| v.wrapping_mul(t.weight())))
            .fold(0i32, i32::wrapping_add)
    }

    fn unpack(&self) -> impl Iterator<Item = (GossipType, i32)> + '_ {
        GossipType::ALL
            .into_iter()
            .filter_map(|t| self.entries[t.ordinal()].map(|v| (t, v)))
    }

    fn decay(&mut self) {
        for t in GossipType::ALL {
            if let Some(v) = self.entries[t.ordinal()] {
                let new_value = v.wrapping_sub(t.decay_per_day());
                self.entries[t.ordinal()] = if new_value < DISCARD_THRESHOLD {
                    None
                } else {
                    Some(new_value)
                };
            }
        }
    }

    fn is_empty(&self) -> bool {
        self.entries.iter().all(Option::is_none)
    }

    fn make_sure_value_isnt_too_low_or_too_high(&mut self, t: GossipType) {
        let value = self.get_int(t);
        if value > t.max() {
            self.entries[t.ordinal()] = Some(t.max());
        }
        if value < DISCARD_THRESHOLD {
            self.entries[t.ordinal()] = None;
        }
    }
}

/// One `GossipEntry` (target, type, value).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GossipEntry {
    pub target: Uuid,
    pub gossip_type: GossipType,
    pub value: i32,
}

impl GossipEntry {
    /// `weightedValue()`.
    pub fn weighted_value(&self) -> i32 {
        self.value.wrapping_mul(self.gossip_type.weight())
    }
}

#[derive(Clone, Default, Debug)]
pub struct GossipContainer {
    /// Insertion-ordered; the Java iteration order is derived with [`GossipContainer::order`].
    gossips: Vec<(Uuid, EntityGossips)>,
    /// Length of the Java `HashMap` table (0 = not allocated); only ever grows.
    table_capacity: usize,
}

impl PartialEq for GossipContainer {
    fn eq(&self, other: &Self) -> bool {
        self.gossips.len() == other.gossips.len()
            && self
                .gossips
                .iter()
                .all(|(k, v)| other.gossips.iter().any(|(k2, v2)| k == k2 && v == v2))
    }
}

impl GossipContainer {
    /// `new GossipContainer()`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Indexes of `gossips` in `HashMap` iteration order.
    fn order(&self) -> Vec<usize> {
        let cap = self.table_capacity.max(1);
        let mut order: Vec<usize> = (0..self.gossips.len()).collect();
        order.sort_by_key(|&i| {
            let h = java_hash_code(&self.gossips[i].0) as u32;
            ((h ^ (h >> 16)) as usize) & (cap - 1)
        });
        order
    }

    fn position(&self, target: &Uuid) -> Option<usize> {
        self.gossips.iter().position(|(k, _)| k == target)
    }

    /// `getOrCreate(target)` = `HashMap.computeIfAbsent`.
    fn get_or_create(&mut self, target: Uuid) -> &mut EntityGossips {
        if let Some(i) = self.position(&target) {
            return &mut self.gossips[i].1;
        }
        if self.table_capacity == 0 {
            self.table_capacity = 16;
        } else if self.gossips.len() > self.table_capacity * 3 / 4 {
            self.table_capacity *= 2;
        }
        self.gossips.push((target, EntityGossips::default()));
        &mut self.gossips.last_mut().expect("just pushed").1
    }

    /// `getGossipEntries()` (debug): per target the type values, in `HashMap` order.
    pub fn get_gossip_entries(&self) -> Vec<(Uuid, Vec<(GossipType, i32)>)> {
        self.order()
            .into_iter()
            .map(|i| (self.gossips[i].0, self.gossips[i].1.unpack().collect()))
            .collect()
    }

    /// `decay()`.
    pub fn decay(&mut self) {
        for (_, g) in &mut self.gossips {
            g.decay();
        }
        self.gossips.retain(|(_, g)| !g.is_empty());
    }

    /// `unpack()`: all entries, targets in `HashMap` order.
    pub fn unpack(&self) -> Vec<GossipEntry> {
        self.order()
            .into_iter()
            .flat_map(|i| {
                self.gossips[i].1.unpack().map(move |(t, v)| GossipEntry {
                    target: self.gossips[i].0,
                    gossip_type: t,
                    value: v,
                })
            })
            .collect()
    }

    /// `selectGossipsForTransfer(random, maxCount)`: the distinct selected entries (in first-selection order).
    fn select_gossips_for_transfer(
        &self,
        random: &mut dyn RandomSource,
        max_count: i32,
    ) -> Vec<GossipEntry> {
        let entries = self.unpack();
        if entries.is_empty() {
            return Vec::new();
        }
        let mut ranges = Vec::with_capacity(entries.len());
        let mut ranges_end = 0i32;
        for e in &entries {
            ranges_end = ranges_end.wrapping_add(e.weighted_value().wrapping_abs());
            ranges.push(ranges_end.wrapping_sub(1));
        }
        let mut selected: Vec<usize> = Vec::new();
        for _ in 0..max_count {
            let choice = random.next_int_bounded(ranges_end);
            // `Arrays.binarySearch`: found index, else the insertion point
            let index = match ranges.binary_search(&choice) {
                Ok(i) => i,
                Err(i) => i,
            };
            if !selected.contains(&index) {
                selected.push(index);
            }
        }
        selected.into_iter().map(|i| entries[i]).collect()
    }

    /// `transferFrom(source, random, maxCount)`: returns the number of gossips selected.
    pub fn transfer_from(
        &mut self,
        source: &GossipContainer,
        random: &mut dyn RandomSource,
        max_count: i32,
    ) -> usize {
        let new_gossips = source.select_gossips_for_transfer(random, max_count);
        for g in &new_gossips {
            let decayed = g.value.wrapping_sub(g.gossip_type.decay_per_transfer());
            if decayed >= DISCARD_THRESHOLD {
                let entry = &mut self.get_or_create(g.target).entries[g.gossip_type.ordinal()];
                *entry = Some(match *entry {
                    // `GossipContainer::mergeValuesForTransfer`
                    Some(old) => old.max(decayed),
                    None => decayed,
                });
            }
        }
        new_gossips.len()
    }

    /// `getReputation(entity, types)`.
    pub fn get_reputation(&self, entity: &Uuid, types: impl Fn(GossipType) -> bool) -> i32 {
        self.position(entity)
            .map_or(0, |i| self.gossips[i].1.weighted_value(&types))
    }

    /// `getCountForType(type, valueTest)`.
    pub fn get_count_for_type(&self, t: GossipType, value_test: impl Fn(f64) -> bool) -> i64 {
        self.gossips
            .iter()
            .filter(|(_, g)| value_test(f64::from(g.get_int(t).wrapping_mul(t.weight()))))
            .count() as i64
    }

    /// `add(target, type, amountToAdd)`.
    pub fn add(&mut self, target: Uuid, t: GossipType, amount_to_add: i32) {
        let gossips = self.get_or_create(target);
        let slot = &mut gossips.entries[t.ordinal()];
        *slot = Some(match *slot {
            // `mergeValuesForAddition`
            Some(old) => {
                let sum = old.wrapping_add(amount_to_add);
                if sum > t.max() { t.max().max(old) } else { sum }
            }
            None => amount_to_add,
        });
        gossips.make_sure_value_isnt_too_low_or_too_high(t);
        if gossips.is_empty() {
            self.remove_target(&target);
        }
    }

    fn remove_target(&mut self, target: &Uuid) {
        if let Some(i) = self.position(target) {
            self.gossips.remove(i);
        }
    }

    /// `remove(target, type, amountToRemove)`.
    pub fn remove_amount(&mut self, target: Uuid, t: GossipType, amount_to_remove: i32) {
        self.add(target, t, amount_to_remove.wrapping_neg());
    }

    /// `remove(target, type)`.
    pub fn remove(&mut self, target: &Uuid, t: GossipType) {
        if let Some(i) = self.position(target) {
            self.gossips[i].1.entries[t.ordinal()] = None;
            if self.gossips[i].1.is_empty() {
                self.gossips.remove(i);
            }
        }
    }

    /// `remove(type)`: that type from every target.
    pub fn remove_type(&mut self, t: GossipType) {
        for (_, g) in &mut self.gossips {
            g.entries[t.ordinal()] = None;
        }
        self.gossips.retain(|(_, g)| !g.is_empty());
    }

    /// `clear()`.
    pub fn clear(&mut self) {
        self.gossips.clear();
    }

    /// `putAll(container)`.
    pub fn put_all(&mut self, container: &GossipContainer) {
        for i in container.order() {
            let (target, source) = &container.gossips[i];
            let dest = self.get_or_create(*target);
            for (t, v) in source.unpack() {
                dest.entries[t.ordinal()] = Some(v);
            }
        }
    }

    /// `copy()`.
    pub fn copy(&self) -> GossipContainer {
        let mut container = GossipContainer::new();
        container.put_all(self);
        container
    }

    pub fn is_empty(&self) -> bool {
        self.gossips.is_empty()
    }

    /// `GossipContainer.CODEC` encode: a list of `{Target, Type, Value}`.
    pub fn to_nbt(&self) -> ListTag {
        let mut list = ListTag::new();
        for e in self.unpack() {
            let mut c = CompoundTag::new();
            c.put(
                "Target",
                Tag::IntArray(uuid_to_int_array(&e.target).to_vec()),
            );
            c.put_string("Type", e.gossip_type.id());
            c.put_int("Value", e.value);
            list.push(Tag::Compound(c));
        }
        list
    }

    /// `GossipContainer.CODEC` decode: invalid entries are dropped (the codec's partial result), valid ones are kept
    /// (`Value` must be positive, `Target` a UUID int array, `Type` a known name).
    pub fn from_nbt(list: &ListTag) -> GossipContainer {
        let mut container = GossipContainer::new();
        for tag in list.iter() {
            let Some(c) = tag.as_compound() else { continue };
            let Some(target) = c.get("Target").and_then(|t| match t {
                Tag::IntArray(a) => <[i32; 4]>::try_from(a.clone())
                    .ok()
                    .map(uuid_from_int_array),
                Tag::List(l) => l
                    .iter()
                    .map(|t| t.as_number().map(|n| n.int_value()))
                    .collect::<Option<Vec<i32>>>()
                    .and_then(|v| <[i32; 4]>::try_from(v).ok())
                    .map(uuid_from_int_array),
                _ => None,
            }) else {
                continue;
            };
            let Some(gossip_type) = c.get_string("Type").and_then(GossipType::by_id) else {
                continue;
            };
            let Some(value) = c
                .get("Value")
                .and_then(Tag::as_number)
                .map(|n| n.int_value())
                .filter(|&v| v > 0)
            else {
                continue;
            };
            container.get_or_create(target).entries[gossip_type.ordinal()] = Some(value);
        }
        container
    }
}
