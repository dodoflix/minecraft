//! `net.minecraft.nbt.ListTag`: heterogeneous list (26.x), wrapping non-compound elements into
//! `{"": element}` compounds on disk when the element types differ.

use crate::CompoundTag;
use crate::tag::Tag;
use std::ops::Deref;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ListTag {
    list: Vec<Tag>,
}

impl Deref for ListTag {
    type Target = [Tag];
    fn deref(&self) -> &[Tag] {
        &self.list
    }
}

/// `ListTag.isWrapper`: a compound with exactly the key `""`.
pub(crate) fn is_wrapper(tag: &CompoundTag) -> bool {
    tag.len() == 1 && tag.contains("")
}

impl ListTag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            list: Vec::with_capacity(capacity),
        }
    }

    pub(crate) fn size_in_bytes(&self) -> i32 {
        36 + 4 * self.list.len() as i32 + self.list.iter().map(Tag::size_in_bytes).sum::<i32>()
    }

    /// `identifyRawElementType()`: the common element id, or 10 (compound) when mixed.
    pub fn identify_raw_element_type(&self) -> u8 {
        let mut homogenous = 0u8;
        for element in &self.list {
            let id = element.id();
            if homogenous == 0 {
                homogenous = id;
            } else if homogenous != id {
                return 10;
            }
        }
        homogenous
    }

    /// `add(tag)`.
    pub fn push(&mut self, tag: impl Into<Tag>) {
        self.list.push(tag.into());
    }

    /// `add(index, tag)`.
    pub fn insert(&mut self, index: usize, tag: impl Into<Tag>) {
        self.list.insert(index, tag.into());
    }

    /// `set(index, tag)`, returning the previous element.
    pub fn set(&mut self, index: usize, tag: impl Into<Tag>) -> Tag {
        std::mem::replace(&mut self.list[index], tag.into())
    }

    pub fn remove(&mut self, index: usize) -> Tag {
        self.list.remove(index)
    }

    pub fn clear(&mut self) {
        self.list.clear();
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut Tag> {
        self.list.get_mut(index)
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, Tag> {
        self.list.iter_mut()
    }

    /// `addAndUnwrap(tag)`: a `{"": x}` compound is stored as `x`.
    pub fn add_and_unwrap(&mut self, tag: Tag) {
        match tag {
            Tag::Compound(mut c) if c.len() == 1 && c.contains("") => {
                let inner = c.remove("").unwrap();
                self.list.push(inner);
            }
            other => self.list.push(other),
        }
    }

    pub fn get_compound(&self, index: usize) -> Option<&CompoundTag> {
        self.get(index).and_then(Tag::as_compound)
    }
    pub fn get_compound_or_empty(&self, index: usize) -> CompoundTag {
        self.get_compound(index).cloned().unwrap_or_default()
    }
    pub fn get_list(&self, index: usize) -> Option<&ListTag> {
        self.get(index).and_then(Tag::as_list)
    }
    pub fn get_list_or_empty(&self, index: usize) -> ListTag {
        self.get_list(index).cloned().unwrap_or_default()
    }
    pub fn get_short(&self, index: usize) -> Option<i16> {
        self.get(index).and_then(Tag::as_short)
    }
    pub fn get_short_or(&self, index: usize, default: i16) -> i16 {
        self.get_short(index).unwrap_or(default)
    }
    pub fn get_int(&self, index: usize) -> Option<i32> {
        self.get(index).and_then(Tag::as_int)
    }
    pub fn get_int_or(&self, index: usize, default: i32) -> i32 {
        self.get_int(index).unwrap_or(default)
    }
    pub fn get_int_array(&self, index: usize) -> Option<&[i32]> {
        self.get(index).and_then(Tag::as_int_array)
    }
    pub fn get_long_array(&self, index: usize) -> Option<&[i64]> {
        self.get(index).and_then(Tag::as_long_array)
    }
    pub fn get_double(&self, index: usize) -> Option<f64> {
        self.get(index).and_then(Tag::as_double)
    }
    pub fn get_double_or(&self, index: usize, default: f64) -> f64 {
        self.get_double(index).unwrap_or(default)
    }
    pub fn get_float(&self, index: usize) -> Option<f32> {
        self.get(index).and_then(Tag::as_float)
    }
    pub fn get_float_or(&self, index: usize, default: f32) -> f32 {
        self.get_float(index).unwrap_or(default)
    }
    pub fn get_string(&self, index: usize) -> Option<&str> {
        self.get(index).and_then(Tag::as_string)
    }
    pub fn get_string_or<'a>(&'a self, index: usize, default: &'a str) -> &'a str {
        self.get_string(index).unwrap_or(default)
    }

    /// `compoundStream()`.
    pub fn compounds(&self) -> impl Iterator<Item = &CompoundTag> {
        self.list.iter().filter_map(Tag::as_compound)
    }
}

impl From<Vec<Tag>> for ListTag {
    fn from(list: Vec<Tag>) -> Self {
        Self { list }
    }
}

impl FromIterator<Tag> for ListTag {
    fn from_iter<I: IntoIterator<Item = Tag>>(iter: I) -> Self {
        Self {
            list: iter.into_iter().collect(),
        }
    }
}

impl IntoIterator for ListTag {
    type Item = Tag;
    type IntoIter = std::vec::IntoIter<Tag>;
    fn into_iter(self) -> Self::IntoIter {
        self.list.into_iter()
    }
}

impl<'a> IntoIterator for &'a ListTag {
    type Item = &'a Tag;
    type IntoIter = std::slice::Iter<'a, Tag>;
    fn into_iter(self) -> Self::IntoIter {
        self.list.iter()
    }
}
