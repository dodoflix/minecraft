//! `net.minecraft.nbt.CompoundTag`.
//!
//! Vanilla stores a `java.util.HashMap`, so its serialisation order is the HashMap bucket order.
//! This type iterates in insertion order (`iter`, `keys`, `values`) but remembers the table
//! capacity a Java `HashMap` would have, so that [`CompoundTag::iter_java_order`] (used for binary
//! output) reproduces vanilla's byte-exact key order.
//!
//! A bin of 9 or more keys is a red-black tree in Java whose iteration order is not the insertion order, and
//! where a tree forms depends on how the table grew. A compound that held 9 keys at once therefore keeps a
//! [`History`] (the tick of every put, the removed keys with their put and remove tick) and its order is the replay of
//! all of it on [`mc_core::hash_bins::Bins`]. Below 9 keys no bin can be a tree and the order is the stable sort by
//! bucket.
//!
//! shortcut: the history of a compound grows with every put of a new key and every remove for as long as it lives, and
//! every `iter_java_order` replays it all. Upgrade when such compounds are edited so often that this shows.

use crate::ListTag;
use crate::tag::{Number, Tag};
use indexmap::{Equivalent, IndexMap};
use mc_core::hash_bins::Bins;
use mc_core::java_string::JavaString;
use std::hash::{Hash, Hasher};
use std::sync::LazyLock;

#[derive(Debug, Default)]
pub struct CompoundTag {
    tags: IndexMap<JavaString, Tag>,
    /// Capacity of the Java `HashMap` table (0 = not yet allocated).
    cap: u32,
    /// The table size the first `resize` made for a map built by `new HashMap<>(map)` (0 = a fresh map: 16).
    init_cap: u32,
    /// The entries are in the iteration order of a map whose history is unknown (`from_hash_order`): no replay.
    frozen: bool,
    /// Puts and removes since the compound first held 9 keys (see the module docs).
    history: Option<Box<History>>,
}

/// What a replay needs of a compound that held 9 keys at once: its puts and removes in order. A tick is one put of a
/// new key or one remove.
#[derive(Clone, Debug)]
struct History {
    clock: u32,
    /// Tick of the put of each key of `tags`, in `tags` order.
    stamps: Vec<u32>,
    /// Removed keys: (key, tick of its put, tick of its remove).
    removed: Vec<(JavaString, u32, u32)>,
}

impl History {
    /// The `n` keys of `tags`, put in this order on the table the compound started with.
    fn of_puts(n: usize) -> Box<History> {
        Box::new(History {
            clock: n as u32,
            stamps: (0..n as u32).collect(),
            removed: Vec::new(),
        })
    }
}

static EMPTY_COMPOUND: LazyLock<CompoundTag> = LazyLock::new(CompoundTag::new);
static EMPTY_LIST: LazyLock<ListTag> = LazyLock::new(ListTag::new);

/// A lookup by text: finds the key of a [`JavaString`] without lone surrogates that has this text, and hashes like it.
struct Name<'a>(&'a str);

impl Hash for Name<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl Equivalent<JavaString> for Name<'_> {
    fn equivalent(&self, key: &JavaString) -> bool {
        *key == *self.0
    }
}

/// `String.hashCode()` followed by `HashMap.hash` spreading.
fn java_hash(key: &JavaString) -> u32 {
    let h = key.java_hash_code();
    (h ^ ((h as u32) >> 16) as i32) as u32
}

/// Table capacity after `len` organic `put`s into a fresh `new HashMap<>()`.
fn organic_cap(len: usize) -> u32 {
    if len == 0 {
        return 0;
    }
    let mut cap = 16u32;
    while len > (cap as usize) * 3 / 4 {
        cap *= 2;
    }
    cap
}

/// Do 9 of `keys` share a bucket of a table of `first_cap` buckets? A tree bin in a later table needs that (tables only
/// grow).
fn has_bin_of_nine<'a>(keys: impl Iterator<Item = &'a JavaString>, first_cap: usize) -> bool {
    let mut counts = vec![0u32; first_cap];
    for k in keys {
        let bucket = java_hash(k) as usize & (first_cap - 1);
        counts[bucket] += 1;
        if counts[bucket] == 9 {
            return true;
        }
    }
    false
}

impl CompoundTag {
    pub fn new() -> Self {
        Self::default()
    }

    /// `size()`.
    pub fn len(&self) -> usize {
        self.tags.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }

    pub(crate) fn size_in_bytes(&self) -> i32 {
        let mut size = 48i32;
        for (k, v) in &self.tags {
            size += 28 + 2 * k.utf16_len() as i32;
            size += 36;
            size += v.size_in_bytes();
        }
        size
    }

    /// Insertion-ordered iteration.
    pub fn iter(&self) -> indexmap::map::Iter<'_, JavaString, Tag> {
        self.tags.iter()
    }

    pub fn iter_mut(&mut self) -> indexmap::map::IterMut<'_, JavaString, Tag> {
        self.tags.iter_mut()
    }

    pub fn keys(&self) -> indexmap::map::Keys<'_, JavaString, Tag> {
        self.tags.keys()
    }

    pub fn values(&self) -> indexmap::map::Values<'_, JavaString, Tag> {
        self.tags.values()
    }

    pub fn values_mut(&mut self) -> indexmap::map::ValuesMut<'_, JavaString, Tag> {
        self.tags.values_mut()
    }

    /// Entries in the order a vanilla `HashMap` would iterate them (bucket index, then insertion; the keys of a tree
    /// bin in the order of its `next` chain).
    pub fn iter_java_order(&self) -> Vec<(&JavaString, &Tag)> {
        self.java_order()
            .into_iter()
            .filter_map(|i| self.tags.get_index(i))
            .collect()
    }

    /// Indices into `tags` in the iteration order of the `HashMap`.
    fn java_order(&self) -> Vec<usize> {
        if self.cap == 0 {
            return Vec::new();
        }
        if let Some(history) = &self.history {
            let first_cap = if self.init_cap == 0 {
                16
            } else {
                self.init_cap as usize
            };
            let keys = self
                .tags
                .keys()
                .chain(history.removed.iter().map(|(key, ..)| key));
            // a tree needs 9 live keys in one bucket; without that the table grew with the key count and the sort is exact
            if has_bin_of_nine(keys, first_cap) {
                return self.replay(history);
            }
        }
        let mask = self.cap - 1;
        let mut order: Vec<usize> = (0..self.tags.len()).collect();
        // stable: insertion order within a bucket
        order.sort_by_cached_key(|&i| {
            self.tags
                .get_index(i)
                .map_or(0, |(k, _)| java_hash(k) & mask)
        });
        order
    }

    /// The iteration order (indices into `tags`) after every put and remove of `history` on the model of the table.
    fn replay(&self, history: &History) -> Vec<usize> {
        let present = self.tags.len();
        let keys: Vec<&JavaString> = self
            .tags
            .keys()
            .chain(history.removed.iter().map(|(key, ..)| key))
            .collect();
        let hashes = keys.iter().map(|k| k.java_hash_code()).collect();
        let compare = |a: usize, b: usize| keys[a].java_cmp(keys[b]);
        // (tick, is a put, index into `keys`)
        let mut events: Vec<(u32, bool, usize)> = history
            .stamps
            .iter()
            .enumerate()
            .map(|(i, &tick)| (tick, true, i))
            .collect();
        for (j, &(_, put, remove)) in history.removed.iter().enumerate() {
            events.push((put, true, present + j));
            events.push((remove, false, present + j));
        }
        events.sort_unstable_by_key(|e| e.0);
        let mut bins = Bins::new(hashes, &compare, self.init_cap as usize);
        for (_, put, key) in events {
            if put {
                bins.put(key);
            } else {
                bins.remove(key);
            }
        }
        bins.order().into_iter().filter(|&i| i < present).collect()
    }

    /// Records the put of the new key that was just added to `tags`.
    fn record_put(&mut self) {
        let len = self.tags.len();
        match &mut self.history {
            Some(history) => {
                history.stamps.push(history.clock);
                history.clock += 1;
            }
            // the first 8 keys never made a tree, and the table they sit on is the one the compound started with
            None if len == 9 && !self.frozen => self.history = Some(History::of_puts(9)),
            None => {}
        }
    }

    /// A compound whose `HashMap` iterates `entries` in exactly this order (the entries are the iteration order of a
    /// vanilla compound whose table capacity is unknown, for example the result of a `Dynamic` rewrite). The table
    /// capacity is the one of an organically filled map when that yields the order, else the smallest that does, so the
    /// binary form lists the entries as given.
    pub fn from_hash_order<K: Into<JavaString>>(
        entries: impl IntoIterator<Item = (K, Tag)>,
    ) -> Self {
        let tags: IndexMap<JavaString, Tag> =
            entries.into_iter().map(|(k, t)| (k.into(), t)).collect();
        let hashes: Vec<u32> = tags.keys().map(java_hash).collect();
        let fits = |cap: u32| {
            hashes.len() * 4 <= cap as usize * 3
                && hashes
                    .windows(2)
                    .all(|w| (w[0] & (cap - 1)) <= (w[1] & (cap - 1)))
        };
        // the table of a map filled by `put`s is preferred: a later `put` then orders new keys like vanilla
        let organic = organic_cap(hashes.len()).max(1);
        let mut cap = 1u32;
        if fits(organic) {
            cap = organic;
        } else {
            while cap <= (1 << 30) && !fits(cap) {
                cap *= 2;
            }
        }
        CompoundTag {
            cap: if tags.is_empty() { 0 } else { cap },
            tags,
            init_cap: 0,
            // the given order may be a tree bin's: replaying the keys would reorder it
            frozen: true,
            history: None,
        }
    }

    /// `put(name, tag)`, returning the previous value.
    pub fn put(&mut self, name: impl Into<JavaString>, tag: impl Into<Tag>) -> Option<Tag> {
        let (_, old) = self.tags.insert_full(name.into(), tag.into());
        if old.is_none() {
            if self.cap == 0 {
                self.cap = 16;
            }
            if self.tags.len() > (self.cap as usize) * 3 / 4 {
                self.cap *= 2;
            }
            self.record_put();
        }
        old
    }

    pub fn put_byte(&mut self, name: impl Into<JavaString>, value: i8) {
        self.put(name, Tag::Byte(value));
    }
    pub fn put_short(&mut self, name: impl Into<JavaString>, value: i16) {
        self.put(name, Tag::Short(value));
    }
    pub fn put_int(&mut self, name: impl Into<JavaString>, value: i32) {
        self.put(name, Tag::Int(value));
    }
    pub fn put_long(&mut self, name: impl Into<JavaString>, value: i64) {
        self.put(name, Tag::Long(value));
    }
    pub fn put_float(&mut self, name: impl Into<JavaString>, value: f32) {
        self.put(name, Tag::float(value));
    }
    pub fn put_double(&mut self, name: impl Into<JavaString>, value: f64) {
        self.put(name, Tag::double(value));
    }
    pub fn put_string(&mut self, name: impl Into<JavaString>, value: impl Into<JavaString>) {
        self.put(name, Tag::String(value.into()));
    }
    pub fn put_byte_array(&mut self, name: impl Into<JavaString>, value: Vec<i8>) {
        self.put(name, Tag::ByteArray(value));
    }
    pub fn put_int_array(&mut self, name: impl Into<JavaString>, value: Vec<i32>) {
        self.put(name, Tag::IntArray(value));
    }
    pub fn put_long_array(&mut self, name: impl Into<JavaString>, value: Vec<i64>) {
        self.put(name, Tag::LongArray(value));
    }
    pub fn put_boolean(&mut self, name: impl Into<JavaString>, value: bool) {
        self.put(name, Tag::from_bool(value));
    }

    /// Builder form of [`put`](Self::put).
    pub fn with(mut self, name: impl Into<JavaString>, tag: impl Into<Tag>) -> Self {
        self.put(name, tag);
        self
    }

    pub fn get(&self, name: &str) -> Option<&Tag> {
        self.tags.get(&Name(name))
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Tag> {
        self.tags.get_mut(&Name(name))
    }

    /// `get(name)` for a name that may hold lone surrogates.
    pub fn get_java(&self, name: &JavaString) -> Option<&Tag> {
        self.tags.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.tags.contains_key(&Name(name))
    }

    fn number(&self, name: &str) -> Option<Number> {
        self.get(name).and_then(Tag::as_number)
    }

    pub fn get_byte(&self, name: &str) -> Option<i8> {
        self.number(name).map(Number::byte_value)
    }
    pub fn get_byte_or(&self, name: &str, default: i8) -> i8 {
        self.get_byte(name).unwrap_or(default)
    }
    pub fn get_short(&self, name: &str) -> Option<i16> {
        self.number(name).map(Number::short_value)
    }
    pub fn get_short_or(&self, name: &str, default: i16) -> i16 {
        self.get_short(name).unwrap_or(default)
    }
    pub fn get_int(&self, name: &str) -> Option<i32> {
        self.number(name).map(Number::int_value)
    }
    pub fn get_int_or(&self, name: &str, default: i32) -> i32 {
        self.get_int(name).unwrap_or(default)
    }
    pub fn get_long(&self, name: &str) -> Option<i64> {
        self.number(name).map(Number::long_value)
    }
    pub fn get_long_or(&self, name: &str, default: i64) -> i64 {
        self.get_long(name).unwrap_or(default)
    }
    pub fn get_float(&self, name: &str) -> Option<f32> {
        self.number(name).map(Number::float_value)
    }
    pub fn get_float_or(&self, name: &str, default: f32) -> f32 {
        self.get_float(name).unwrap_or(default)
    }
    pub fn get_double(&self, name: &str) -> Option<f64> {
        self.number(name).map(Number::double_value)
    }
    pub fn get_double_or(&self, name: &str, default: f64) -> f64 {
        self.get_double(name).unwrap_or(default)
    }
    pub fn get_boolean(&self, name: &str) -> Option<bool> {
        self.number(name).map(Number::boolean_value)
    }
    pub fn get_boolean_or(&self, name: &str, default: bool) -> bool {
        self.get_byte_or(name, default as i8) != 0
    }
    pub fn get_string(&self, name: &str) -> Option<&str> {
        self.get(name).and_then(Tag::as_string)
    }
    /// The exact string, lone surrogates included.
    pub fn get_java_string(&self, name: &str) -> Option<&JavaString> {
        match self.get(name)? {
            Tag::String(s) => Some(s),
            _ => None,
        }
    }
    pub fn get_string_or<'a>(&'a self, name: &str, default: &'a str) -> &'a str {
        self.get_string(name).unwrap_or(default)
    }
    pub fn get_byte_array(&self, name: &str) -> Option<&[i8]> {
        self.get(name).and_then(Tag::as_byte_array)
    }
    pub fn get_int_array(&self, name: &str) -> Option<&[i32]> {
        self.get(name).and_then(Tag::as_int_array)
    }
    pub fn get_long_array(&self, name: &str) -> Option<&[i64]> {
        self.get(name).and_then(Tag::as_long_array)
    }
    pub fn get_compound(&self, name: &str) -> Option<&CompoundTag> {
        self.get(name).and_then(Tag::as_compound)
    }
    pub fn get_compound_mut(&mut self, name: &str) -> Option<&mut CompoundTag> {
        self.get_mut(name).and_then(Tag::as_compound_mut)
    }
    /// `getCompoundOrEmpty` (shared empty instance when absent).
    pub fn get_compound_or_empty(&self, name: &str) -> &CompoundTag {
        self.get_compound(name).unwrap_or(&EMPTY_COMPOUND)
    }
    pub fn get_list(&self, name: &str) -> Option<&ListTag> {
        self.get(name).and_then(Tag::as_list)
    }
    pub fn get_list_mut(&mut self, name: &str) -> Option<&mut ListTag> {
        self.get_mut(name).and_then(Tag::as_list_mut)
    }
    /// `getListOrEmpty` (shared empty instance when absent).
    pub fn get_list_or_empty(&self, name: &str) -> &ListTag {
        self.get_list(name).unwrap_or(&EMPTY_LIST)
    }

    /// `remove(name)`, preserving the order of the remaining entries.
    pub fn remove(&mut self, name: &str) -> Option<Tag> {
        let (index, key, tag) = self.tags.shift_remove_full(&Name(name))?;
        if let Some(history) = &mut self.history {
            let put = history.stamps.remove(index);
            history.removed.push((key, put, history.clock));
            history.clock += 1;
        }
        Some(tag)
    }

    /// `copy()`: deep copy, reproducing the key order of a Java copy (entries re-inserted in the
    /// source's HashMap iteration order into a fresh map).
    pub fn copy(&self) -> CompoundTag {
        self.clone()
    }

    /// A second handle on this compound for code where Java passes the object itself (a list of `CompoundTag`
    /// references, a field kept as read): every table stays as it is, where [`Clone`] (`copy()`) re-inserts each
    /// level into a fresh table and so can reorder the keys.
    pub fn share(&self) -> CompoundTag {
        CompoundTag {
            tags: self
                .tags
                .iter()
                .map(|(k, v)| (k.clone(), v.share()))
                .collect(),
            cap: self.cap,
            init_cap: self.init_cap,
            frozen: self.frozen,
            history: self.history.clone(),
        }
    }

    /// `shallowCopy()`: `new HashMap<>(tags)`; the table is sized from the entry count.
    pub fn shallow_copy(&self) -> CompoundTag {
        let mut out = CompoundTag::new();
        let n = self.len();
        if n > 0 {
            // `HashMap.putMapEntries` (JDK 19+): `ceil(size / 0.75)` rounded up to a power of two
            let t = (n as f64 / 0.75f64).ceil() as u32;
            out.cap = t.max(1).next_power_of_two();
        }
        out.init_cap = out.cap;
        for (k, v) in self.iter_java_order() {
            out.tags.insert(k.clone(), v.clone());
        }
        out.history = (n >= 9).then(|| History::of_puts(n));
        out
    }

    /// `shallowCopy()` of an owned compound: the same table sizing and iteration order as [`Self::shallow_copy`], with the
    /// values moved (a shallow copy shares them in Java).
    pub fn into_shallow_copy(self) -> CompoundTag {
        let n = self.tags.len();
        let cap = if n > 0 {
            ((n as f64 / 0.75f64).ceil() as u32)
                .max(1)
                .next_power_of_two()
        } else {
            0
        };
        let order = self.java_order();
        let mut slots: Vec<Option<(JavaString, Tag)>> = self.tags.into_iter().map(Some).collect();
        CompoundTag {
            cap,
            init_cap: cap,
            frozen: false,
            history: (n >= 9).then(|| History::of_puts(n)),
            tags: order.into_iter().filter_map(|i| slots[i].take()).collect(),
        }
    }

    /// `merge(other)`: recursively merges compounds, otherwise `other`'s values win.
    pub fn merge(&mut self, other: &CompoundTag) -> &mut Self {
        for (name, other_tag) in other.iter_java_order() {
            if let (Tag::Compound(o), Some(Tag::Compound(s))) = (other_tag, self.tags.get_mut(name))
            {
                s.merge(o);
            } else {
                self.put(name.clone(), other_tag.clone());
            }
        }
        self
    }
}

impl Clone for CompoundTag {
    /// Vanilla `copy()` semantics (see [`CompoundTag::copy`]).
    fn clone(&self) -> Self {
        let len = self.tags.len();
        let cap = organic_cap(len);
        // no remove since the history began and no bin of 9: the table is the organic one and no bin was ever a tree
        let plain = self.history.as_ref().is_none_or(|h| h.removed.is_empty());
        let bin_of_nine = len >= 9 && has_bin_of_nine(self.tags.keys(), 16);
        if cap == self.cap && !self.frozen && self.init_cap == 0 && plain && !bin_of_nine {
            // Same table size: Java's re-insertion keeps every bucket's relative order.
            return CompoundTag {
                tags: self.tags.clone(),
                cap,
                init_cap: 0,
                frozen: false,
                history: (len >= 9).then(|| History::of_puts(len)),
            };
        }
        let mut out = CompoundTag::new();
        for (k, v) in self.iter_java_order() {
            out.put(k.clone(), v.clone());
        }
        out
    }
}

impl PartialEq for CompoundTag {
    fn eq(&self, other: &Self) -> bool {
        self.tags == other.tags
    }
}

impl<'a> IntoIterator for &'a CompoundTag {
    type Item = (&'a JavaString, &'a Tag);
    type IntoIter = indexmap::map::Iter<'a, JavaString, Tag>;
    fn into_iter(self) -> Self::IntoIter {
        self.tags.iter()
    }
}

impl<K: Into<JavaString>, T: Into<Tag>> FromIterator<(K, T)> for CompoundTag {
    fn from_iter<I: IntoIterator<Item = (K, T)>>(iter: I) -> Self {
        let mut c = CompoundTag::new();
        for (k, v) in iter {
            c.put(k, v);
        }
        c
    }
}

/// Builds a [`CompoundTag`]: `compound! { "a" => 1i32, "b" => "x" }`.
#[macro_export]
macro_rules! compound {
    ($($k:expr => $v:expr),* $(,)?) => {{
        #[allow(unused_mut, reason = "an empty `compound! {}` never mutates; the macro cannot tell")]
        let mut c = $crate::CompoundTag::new();
        $( c.put($k, $v); )*
        c
    }};
}
