//! `net.minecraft.nbt.CompoundTag`.
//!
//! Vanilla stores a `java.util.HashMap`, so its serialisation order is the HashMap bucket order.
//! This type iterates in insertion order (`iter`, `keys`, `values`) but remembers the table
//! capacity a Java `HashMap` would have, so that [`CompoundTag::iter_java_order`] (used for binary
//! output) reproduces vanilla's byte-exact key order.
//!
//! shortcut: bins with 9+ colliding keys are treeified by Java and iterate differently; that is
//! not modelled (needs >8 keys with the same bucket, upgrade if byte-identical files demand it).

use crate::tag::{Number, Tag};
use crate::ListTag;
use indexmap::IndexMap;
use std::sync::LazyLock;

#[derive(Debug, Default)]
pub struct CompoundTag {
    tags: IndexMap<String, Tag>,
    /// Capacity of the Java `HashMap` table (0 = not yet allocated).
    cap: u32,
}

static EMPTY_COMPOUND: LazyLock<CompoundTag> = LazyLock::new(CompoundTag::new);
static EMPTY_LIST: LazyLock<ListTag> = LazyLock::new(ListTag::new);

/// `String.hashCode()` followed by `HashMap.hash` spreading.
fn java_hash(key: &str) -> u32 {
    let mut h: i32 = 0;
    for u in key.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(u as i32);
    }
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
            size += 28 + 2 * k.encode_utf16().count() as i32;
            size += 36;
            size += v.size_in_bytes();
        }
        size
    }

    /// Insertion-ordered iteration.
    pub fn iter(&self) -> indexmap::map::Iter<'_, String, Tag> {
        self.tags.iter()
    }

    pub fn iter_mut(&mut self) -> indexmap::map::IterMut<'_, String, Tag> {
        self.tags.iter_mut()
    }

    pub fn keys(&self) -> indexmap::map::Keys<'_, String, Tag> {
        self.tags.keys()
    }

    pub fn values(&self) -> indexmap::map::Values<'_, String, Tag> {
        self.tags.values()
    }

    pub fn values_mut(&mut self) -> indexmap::map::ValuesMut<'_, String, Tag> {
        self.tags.values_mut()
    }

    /// Entries in the order a vanilla `HashMap` would iterate them (bucket index, then insertion).
    pub fn iter_java_order(&self) -> Vec<(&String, &Tag)> {
        let mut v: Vec<(u32, (&String, &Tag))> = if self.cap == 0 {
            Vec::new()
        } else {
            let mask = self.cap - 1;
            self.tags.iter().map(|(k, t)| (java_hash(k) & mask, (k, t))).collect()
        };
        v.sort_by_key(|(bucket, _)| *bucket); // stable: insertion order within a bucket
        v.into_iter().map(|(_, e)| e).collect()
    }

    /// `put(name, tag)`, returning the previous value.
    pub fn put(&mut self, name: impl Into<String>, tag: impl Into<Tag>) -> Option<Tag> {
        let (_, old) = self.tags.insert_full(name.into(), tag.into());
        if old.is_none() {
            if self.cap == 0 {
                self.cap = 16;
            }
            if self.tags.len() > (self.cap as usize) * 3 / 4 {
                self.cap *= 2;
            }
        }
        old
    }

    pub fn put_byte(&mut self, name: impl Into<String>, value: i8) {
        self.put(name, Tag::Byte(value));
    }
    pub fn put_short(&mut self, name: impl Into<String>, value: i16) {
        self.put(name, Tag::Short(value));
    }
    pub fn put_int(&mut self, name: impl Into<String>, value: i32) {
        self.put(name, Tag::Int(value));
    }
    pub fn put_long(&mut self, name: impl Into<String>, value: i64) {
        self.put(name, Tag::Long(value));
    }
    pub fn put_float(&mut self, name: impl Into<String>, value: f32) {
        self.put(name, Tag::float(value));
    }
    pub fn put_double(&mut self, name: impl Into<String>, value: f64) {
        self.put(name, Tag::double(value));
    }
    pub fn put_string(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.put(name, Tag::String(value.into()));
    }
    pub fn put_byte_array(&mut self, name: impl Into<String>, value: Vec<i8>) {
        self.put(name, Tag::ByteArray(value));
    }
    pub fn put_int_array(&mut self, name: impl Into<String>, value: Vec<i32>) {
        self.put(name, Tag::IntArray(value));
    }
    pub fn put_long_array(&mut self, name: impl Into<String>, value: Vec<i64>) {
        self.put(name, Tag::LongArray(value));
    }
    pub fn put_boolean(&mut self, name: impl Into<String>, value: bool) {
        self.put(name, Tag::from_bool(value));
    }

    /// Builder form of [`put`](Self::put).
    pub fn with(mut self, name: impl Into<String>, tag: impl Into<Tag>) -> Self {
        self.put(name, tag);
        self
    }

    pub fn get(&self, name: &str) -> Option<&Tag> {
        self.tags.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Tag> {
        self.tags.get_mut(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.tags.contains_key(name)
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
        self.tags.shift_remove(name)
    }

    /// `copy()`: deep copy, reproducing the key order of a Java copy (entries re-inserted in the
    /// source's HashMap iteration order into a fresh map).
    pub fn copy(&self) -> CompoundTag {
        self.clone()
    }

    /// `shallowCopy()`: `new HashMap<>(tags)`; the table is sized from the entry count.
    pub fn shallow_copy(&self) -> CompoundTag {
        let mut out = CompoundTag::new();
        let n = self.len();
        if n > 0 {
            let t = (n as f32 / 0.75f32 + 1.0f32) as u32;
            out.cap = t.max(1).next_power_of_two();
        }
        for (k, v) in self.iter_java_order() {
            out.tags.insert(k.clone(), v.clone());
        }
        out
    }

    /// `merge(other)`: recursively merges compounds, otherwise `other`'s values win.
    pub fn merge(&mut self, other: &CompoundTag) -> &mut Self {
        for (name, other_tag) in other.iter_java_order() {
            if let (Tag::Compound(o), Some(Tag::Compound(s))) = (other_tag, self.tags.get_mut(name.as_str())) {
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
        let cap = organic_cap(self.tags.len());
        if cap == self.cap {
            // Same table size: Java's re-insertion keeps every bucket's relative order.
            return CompoundTag { tags: self.tags.clone(), cap };
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
    type Item = (&'a String, &'a Tag);
    type IntoIter = indexmap::map::Iter<'a, String, Tag>;
    fn into_iter(self) -> Self::IntoIter {
        self.tags.iter()
    }
}

impl<K: Into<String>, T: Into<Tag>> FromIterator<(K, T)> for CompoundTag {
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
        #[allow(unused_mut)]
        let mut c = $crate::CompoundTag::new();
        $( c.put($k, $v); )*
        c
    }};
}
