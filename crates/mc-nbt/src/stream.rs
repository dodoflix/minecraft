//! Streaming NBT: vanilla's `StreamTagVisitor`, `TagType.parse`/`skip`, `Tag.accept(StreamTagVisitor)`,
//! `NbtIo.parse` and the `nbt.visitors` package (`SkipAll`, `CollectToTag`, `CollectFields`,
//! `SkipFields`, `FieldTree`, `FieldSelector`).
//!
//! Tag types are identified by their numeric id (`TagTypes.getType`); an id above 12 is vanilla's
//! `TagType.createInvalid`, whose `parse`/`skip` fail with `Invalid tag id`.

use crate::accounter::NbtAccounter;
use crate::error::{NbtError, Result};
use crate::io::{read_f32, read_f64, read_i16, read_i32, read_i64, read_i8, read_u16, read_u8, read_utf, skip_bytes, take};
use crate::list::is_wrapper;
use crate::tag::*;
use crate::{compress, CompoundTag, ListTag};
use std::collections::HashMap;

/// `StreamTagVisitor.EntryResult`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryResult {
    Enter,
    Skip,
    Break,
    Halt,
}

/// `StreamTagVisitor.ValueResult`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueResult {
    Continue,
    Break,
    Halt,
}

/// `StreamTagVisitor`. Tag types are passed as ids (see the module docs).
pub trait StreamTagVisitor {
    fn visit_end(&mut self) -> ValueResult;
    fn visit_string(&mut self, value: &str) -> ValueResult;
    fn visit_byte(&mut self, value: i8) -> ValueResult;
    fn visit_short(&mut self, value: i16) -> ValueResult;
    fn visit_int(&mut self, value: i32) -> ValueResult;
    fn visit_long(&mut self, value: i64) -> ValueResult;
    fn visit_float(&mut self, value: f32) -> ValueResult;
    fn visit_double(&mut self, value: f64) -> ValueResult;
    fn visit_byte_array(&mut self, value: &[i8]) -> ValueResult;
    fn visit_int_array(&mut self, value: &[i32]) -> ValueResult;
    fn visit_long_array(&mut self, value: &[i64]) -> ValueResult;
    fn visit_list(&mut self, element_type_id: u8, size: i32) -> ValueResult;
    /// `visitEntry(type)`: a compound entry of this type is about to be read (name not yet known).
    fn visit_entry(&mut self, type_id: u8) -> EntryResult;
    /// `visitEntry(type, id)`: same entry, once its name is known.
    fn visit_entry_named(&mut self, type_id: u8, id: &str) -> EntryResult;
    fn visit_element(&mut self, type_id: u8, index: i32) -> EntryResult;
    fn visit_container_end(&mut self) -> ValueResult;
    fn visit_root_entry(&mut self, type_id: u8) -> ValueResult;
}

// -------------------------------------------------------------------- parse

fn invalid_tag_id(id: u8) -> NbtError {
    NbtError::InvalidTagId(id as i8 as i32)
}

/// `StringTag.skipString`.
fn skip_string(input: &mut &[u8]) -> Result<()> {
    let len = read_u16(input)?;
    skip_bytes(input, len as usize);
    Ok(())
}

/// `DataInput.skipBytes(int)`: non-positive counts skip nothing, a short stream is not an error.
fn skip_i32(input: &mut &[u8], count: i32) {
    if count > 0 {
        skip_bytes(input, count as usize);
    }
}

fn read_list_count(input: &mut &[u8]) -> Result<i32> {
    let count = read_i32(input)?;
    if count < 0 {
        return Err(NbtError::Format(format!("ListTag length cannot be negative: {count}")));
    }
    Ok(count)
}

/// `readString` of `CompoundTag.TYPE.parseCompound` (key accounting).
fn read_key(input: &mut &[u8], acc: &mut NbtAccounter) -> Result<String> {
    let key = read_utf(input)?;
    acc.account_bytes(28)?;
    acc.account_bytes_n(2, key.encode_utf16().count() as i64)?;
    Ok(key)
}

/// `TagType.parse`: reads a payload of type `type_id`, feeding `visitor`.
pub fn parse_type(type_id: u8, input: &mut &[u8], visitor: &mut dyn StreamTagVisitor, acc: &mut NbtAccounter) -> Result<ValueResult> {
    Ok(match type_id {
        TAG_END => {
            acc.account_bytes(8)?;
            visitor.visit_end()
        }
        TAG_BYTE => {
            acc.account_bytes(9)?;
            visitor.visit_byte(read_i8(input)?)
        }
        TAG_SHORT => {
            acc.account_bytes(10)?;
            visitor.visit_short(read_i16(input)?)
        }
        TAG_INT => {
            acc.account_bytes(12)?;
            visitor.visit_int(read_i32(input)?)
        }
        TAG_LONG => {
            acc.account_bytes(16)?;
            visitor.visit_long(read_i64(input)?)
        }
        TAG_FLOAT => {
            acc.account_bytes(12)?;
            visitor.visit_float(read_f32(input)?)
        }
        TAG_DOUBLE => {
            acc.account_bytes(16)?;
            visitor.visit_double(read_f64(input)?)
        }
        TAG_BYTE_ARRAY => {
            acc.account_bytes(24)?;
            let len = read_i32(input)?;
            acc.account_bytes_n(1, len as i64)?;
            let bytes = take(input, len as usize)?;
            visitor.visit_byte_array(&bytes.iter().map(|b| *b as i8).collect::<Vec<_>>())
        }
        TAG_STRING => {
            acc.account_bytes(36)?;
            let s = read_utf(input)?;
            acc.account_bytes_n(2, s.encode_utf16().count() as i64)?;
            visitor.visit_string(&s)
        }
        TAG_LIST => {
            acc.push_depth()?;
            let r = parse_list(input, visitor, acc);
            acc.pop_depth()?;
            r?
        }
        TAG_COMPOUND => {
            acc.push_depth()?;
            let r = parse_compound(input, visitor, acc);
            acc.pop_depth()?;
            r?
        }
        TAG_INT_ARRAY => {
            acc.account_bytes(24)?;
            let len = read_i32(input)?;
            acc.account_bytes_n(4, len as i64)?;
            let bytes = take(input, len as usize * 4)?;
            let data: Vec<i32> = bytes.chunks_exact(4).map(|c| i32::from_be_bytes(c.try_into().unwrap())).collect();
            visitor.visit_int_array(&data)
        }
        TAG_LONG_ARRAY => {
            acc.account_bytes(24)?;
            let len = read_i32(input)?;
            acc.account_bytes_n(8, len as i64)?;
            let bytes = take(input, len as usize * 8)?;
            let data: Vec<i64> = bytes.chunks_exact(8).map(|c| i64::from_be_bytes(c.try_into().unwrap())).collect();
            visitor.visit_long_array(&data)
        }
        _ => return Err(invalid_tag_id(type_id)),
    })
}

fn parse_list(input: &mut &[u8], visitor: &mut dyn StreamTagVisitor, acc: &mut NbtAccounter) -> Result<ValueResult> {
    acc.account_bytes(36)?;
    let element_type = read_u8(input)?;
    let count = read_list_count(input)?;
    match visitor.visit_list(element_type, count) {
        ValueResult::Halt => return Ok(ValueResult::Halt),
        ValueResult::Break => {
            skip_type_n(element_type, input, count, acc)?;
            return Ok(visitor.visit_container_end());
        }
        ValueResult::Continue => {}
    }
    acc.account_bytes_n(4, count as i64)?;
    let mut i = 0;
    while i < count {
        match visitor.visit_element(element_type, i) {
            EntryResult::Halt => return Ok(ValueResult::Halt),
            EntryResult::Skip => skip_type(element_type, input, acc)?,
            EntryResult::Break => {
                skip_type(element_type, input, acc)?;
                break;
            }
            EntryResult::Enter => match parse_type(element_type, input, visitor, acc)? {
                ValueResult::Halt => return Ok(ValueResult::Halt),
                ValueResult::Break => break,
                ValueResult::Continue => {}
            },
        }
        i += 1;
    }
    // After a BREAK the remaining `count - 1 - i` elements are skipped (`i == count` gives -1).
    let to_skip = count - 1 - i;
    if to_skip > 0 {
        skip_type_n(element_type, input, to_skip, acc)?;
    }
    Ok(visitor.visit_container_end())
}

fn parse_compound(input: &mut &[u8], visitor: &mut dyn StreamTagVisitor, acc: &mut NbtAccounter) -> Result<ValueResult> {
    acc.account_bytes(48)?;
    let mut type_id;
    loop {
        type_id = read_u8(input)?;
        if type_id == 0 {
            break;
        }
        match visitor.visit_entry(type_id) {
            EntryResult::Halt => return Ok(ValueResult::Halt),
            EntryResult::Break => {
                skip_string(input)?;
                skip_type(type_id, input, acc)?;
                break;
            }
            EntryResult::Skip => {
                skip_string(input)?;
                skip_type(type_id, input, acc)?;
            }
            EntryResult::Enter => {
                let key = read_key(input, acc)?;
                match visitor.visit_entry_named(type_id, &key) {
                    EntryResult::Halt => return Ok(ValueResult::Halt),
                    EntryResult::Break => {
                        skip_type(type_id, input, acc)?;
                        break;
                    }
                    EntryResult::Skip => skip_type(type_id, input, acc)?,
                    EntryResult::Enter => {
                        acc.account_bytes(36)?;
                        // A BREAK from the value does not stop the compound (matches vanilla).
                        if parse_type(type_id, input, visitor, acc)? == ValueResult::Halt {
                            return Ok(ValueResult::Halt);
                        }
                    }
                }
            }
        }
    }
    if type_id != 0 {
        loop {
            type_id = read_u8(input)?;
            if type_id == 0 {
                break;
            }
            skip_string(input)?;
            skip_type(type_id, input, acc)?;
        }
    }
    Ok(visitor.visit_container_end())
}

/// `TagType.skip(input, accounter)`: skips one payload of type `type_id`.
pub fn skip_type(type_id: u8, input: &mut &[u8], acc: &mut NbtAccounter) -> Result<()> {
    match type_id {
        TAG_END => {}
        TAG_BYTE => skip_i32(input, 1),
        TAG_SHORT => skip_i32(input, 2),
        TAG_INT | TAG_FLOAT => skip_i32(input, 4),
        TAG_LONG | TAG_DOUBLE => skip_i32(input, 8),
        TAG_BYTE_ARRAY => {
            let len = read_i32(input)?;
            skip_i32(input, len);
        }
        TAG_INT_ARRAY => {
            let len = read_i32(input)?;
            skip_i32(input, len.wrapping_mul(4));
        }
        TAG_LONG_ARRAY => {
            let len = read_i32(input)?;
            skip_i32(input, len.wrapping_mul(8));
        }
        TAG_STRING => skip_string(input)?,
        TAG_LIST => {
            acc.push_depth()?;
            let r = (|| -> Result<()> {
                let element_type = read_u8(input)?;
                let count = read_i32(input)?;
                skip_type_n(element_type, input, count, acc)
            })();
            acc.pop_depth()?;
            r?
        }
        TAG_COMPOUND => {
            acc.push_depth()?;
            let r = (|| -> Result<()> {
                loop {
                    let id = read_u8(input)?;
                    if id == 0 {
                        return Ok(());
                    }
                    skip_string(input)?;
                    skip_type(id, input, acc)?;
                }
            })();
            acc.pop_depth()?;
            r?
        }
        _ => return Err(invalid_tag_id(type_id)),
    }
    Ok(())
}

/// `TagType.skip(input, count, accounter)`: skips `count` consecutive payloads.
pub fn skip_type_n(type_id: u8, input: &mut &[u8], count: i32, acc: &mut NbtAccounter) -> Result<()> {
    match type_id {
        TAG_END => Ok(()),
        // StaticSize: `skipBytes(size * count)` with Java int overflow.
        TAG_BYTE | TAG_SHORT | TAG_INT | TAG_FLOAT | TAG_LONG | TAG_DOUBLE => {
            let size = match type_id {
                TAG_BYTE => 1,
                TAG_SHORT => 2,
                TAG_INT | TAG_FLOAT => 4,
                _ => 8,
            };
            skip_i32(input, count.wrapping_mul(size));
            Ok(())
        }
        // VariableSize.
        TAG_BYTE_ARRAY | TAG_STRING | TAG_LIST | TAG_COMPOUND | TAG_INT_ARRAY | TAG_LONG_ARRAY => {
            for _ in 0..count {
                skip_type(type_id, input, acc)?;
            }
            Ok(())
        }
        _ => Err(invalid_tag_id(type_id)),
    }
}

/// `TagType.parseRoot`.
pub fn parse_root(type_id: u8, input: &mut &[u8], visitor: &mut dyn StreamTagVisitor, acc: &mut NbtAccounter) -> Result<()> {
    match visitor.visit_root_entry(type_id) {
        ValueResult::Continue => {
            parse_type(type_id, input, visitor, acc)?;
        }
        ValueResult::Halt => {}
        ValueResult::Break => skip_type(type_id, input, acc)?,
    }
    Ok(())
}

/// `NbtIo.parse`: a named root of any type.
pub fn parse(input: &mut &[u8], visitor: &mut dyn StreamTagVisitor, acc: &mut NbtAccounter) -> Result<()> {
    let type_id = read_u8(input)?;
    if type_id == TAG_END {
        if visitor.visit_root_entry(TAG_END) == ValueResult::Continue {
            visitor.visit_end();
        }
        return Ok(());
    }
    match visitor.visit_root_entry(type_id) {
        ValueResult::Halt => {}
        ValueResult::Break => {
            skip_string(input)?;
            skip_type(type_id, input, acc)?;
        }
        ValueResult::Continue => {
            skip_string(input)?;
            parse_type(type_id, input, visitor, acc)?;
        }
    }
    Ok(())
}

/// `NbtIo.parseCompressed`: gzip-wrapped [`parse`].
pub fn parse_compressed(data: &[u8], visitor: &mut dyn StreamTagVisitor, acc: &mut NbtAccounter) -> Result<()> {
    // Same decompression bound as `nbt_io::read_compressed`.
    let limit = (acc.quota() != i64::MAX).then(|| acc.quota().max(0) as u64);
    let raw = compress::gzip_decompress(data, limit)?;
    parse(&mut raw.as_slice(), visitor, acc)
}

// ------------------------------------------------------------------- accept

/// Shared body of `CompoundTag.accept(StreamTagVisitor)` over `entries` in iteration order.
fn accept_entries<'a>(entries: impl Iterator<Item = (&'a str, &'a Tag)>, visitor: &mut dyn StreamTagVisitor) -> ValueResult {
    for (key, value) in entries {
        let id = value.id();
        match visitor.visit_entry(id) {
            EntryResult::Halt => return ValueResult::Halt,
            EntryResult::Break => return visitor.visit_container_end(),
            EntryResult::Skip => {}
            EntryResult::Enter => match visitor.visit_entry_named(id, key) {
                EntryResult::Halt => return ValueResult::Halt,
                EntryResult::Break => return visitor.visit_container_end(),
                EntryResult::Skip => {}
                EntryResult::Enter => match value.accept_stream(visitor) {
                    ValueResult::Halt => return ValueResult::Halt,
                    ValueResult::Break => return visitor.visit_container_end(),
                    ValueResult::Continue => {}
                },
            },
        }
    }
    visitor.visit_container_end()
}

fn accept_list(list: &ListTag, visitor: &mut dyn StreamTagVisitor) -> ValueResult {
    let element_type = list.identify_raw_element_type();
    match visitor.visit_list(element_type, list.len() as i32) {
        ValueResult::Halt => return ValueResult::Halt,
        ValueResult::Break => return visitor.visit_container_end(),
        ValueResult::Continue => {}
    }
    for (i, element) in list.iter().enumerate() {
        // `wrapIfNeeded`: in a compound-typed list every non-compound / wrapper-shaped element is
        // presented as `{"": element}`.
        let wrapped = element_type == TAG_COMPOUND && !matches!(element, Tag::Compound(c) if !is_wrapper(c));
        let id = if wrapped { TAG_COMPOUND } else { element.id() };
        match visitor.visit_element(id, i as i32) {
            EntryResult::Halt => return ValueResult::Halt,
            EntryResult::Break => return visitor.visit_container_end(),
            EntryResult::Skip => continue,
            EntryResult::Enter => {}
        }
        let result = if wrapped { accept_entries(std::iter::once(("", element)), visitor) } else { element.accept_stream(visitor) };
        match result {
            ValueResult::Halt => return ValueResult::Halt,
            ValueResult::Break => return visitor.visit_container_end(),
            ValueResult::Continue => {}
        }
    }
    visitor.visit_container_end()
}

impl Tag {
    /// `Tag.accept(StreamTagVisitor)`.
    pub fn accept_stream(&self, visitor: &mut dyn StreamTagVisitor) -> ValueResult {
        match self {
            Tag::End => visitor.visit_end(),
            Tag::Byte(v) => visitor.visit_byte(*v),
            Tag::Short(v) => visitor.visit_short(*v),
            Tag::Int(v) => visitor.visit_int(*v),
            Tag::Long(v) => visitor.visit_long(*v),
            Tag::Float(v) => visitor.visit_float(*v),
            Tag::Double(v) => visitor.visit_double(*v),
            Tag::ByteArray(v) => visitor.visit_byte_array(v),
            Tag::String(v) => visitor.visit_string(v),
            Tag::List(l) => accept_list(l, visitor),
            Tag::Compound(c) => accept_entries(c.iter_java_order().into_iter().map(|(k, v)| (k.as_str(), v)), visitor),
            Tag::IntArray(v) => visitor.visit_int_array(v),
            Tag::LongArray(v) => visitor.visit_long_array(v),
        }
    }

    /// `Tag.acceptAsRoot`.
    pub fn accept_as_root(&self, visitor: &mut dyn StreamTagVisitor) {
        if visitor.visit_root_entry(self.id()) == ValueResult::Continue {
            self.accept_stream(visitor);
        }
    }
}

// ------------------------------------------------------------------ SkipAll

/// `SkipAll`: skips every entry and element.
#[derive(Debug, Clone, Copy, Default)]
pub struct SkipAll;

impl StreamTagVisitor for SkipAll {
    fn visit_end(&mut self) -> ValueResult {
        ValueResult::Continue
    }
    fn visit_string(&mut self, _: &str) -> ValueResult {
        ValueResult::Continue
    }
    fn visit_byte(&mut self, _: i8) -> ValueResult {
        ValueResult::Continue
    }
    fn visit_short(&mut self, _: i16) -> ValueResult {
        ValueResult::Continue
    }
    fn visit_int(&mut self, _: i32) -> ValueResult {
        ValueResult::Continue
    }
    fn visit_long(&mut self, _: i64) -> ValueResult {
        ValueResult::Continue
    }
    fn visit_float(&mut self, _: f32) -> ValueResult {
        ValueResult::Continue
    }
    fn visit_double(&mut self, _: f64) -> ValueResult {
        ValueResult::Continue
    }
    fn visit_byte_array(&mut self, _: &[i8]) -> ValueResult {
        ValueResult::Continue
    }
    fn visit_int_array(&mut self, _: &[i32]) -> ValueResult {
        ValueResult::Continue
    }
    fn visit_long_array(&mut self, _: &[i64]) -> ValueResult {
        ValueResult::Continue
    }
    fn visit_list(&mut self, _: u8, _: i32) -> ValueResult {
        ValueResult::Continue
    }
    fn visit_element(&mut self, _: u8, _: i32) -> EntryResult {
        EntryResult::Skip
    }
    fn visit_entry(&mut self, _: u8) -> EntryResult {
        EntryResult::Skip
    }
    fn visit_entry_named(&mut self, _: u8, _: &str) -> EntryResult {
        EntryResult::Skip
    }
    fn visit_container_end(&mut self) -> ValueResult {
        ValueResult::Continue
    }
    fn visit_root_entry(&mut self, _: u8) -> ValueResult {
        ValueResult::Continue
    }
}

// ------------------------------------------------------------ CollectToTag

enum Builder {
    Root(Option<Tag>),
    Compound { compound: CompoundTag, last_id: String },
    List(ListTag),
}

impl Builder {
    fn accept_key(&mut self, id: &str) {
        if let Builder::Compound { last_id, .. } = self {
            last_id.clear();
            last_id.push_str(id);
        }
    }

    fn accept_value(&mut self, tag: Tag) {
        match self {
            Builder::Root(result) => *result = Some(tag),
            Builder::Compound { compound, last_id } => {
                compound.put(last_id.as_str(), tag);
            }
            Builder::List(list) => list.add_and_unwrap(tag),
        }
    }

    fn build(self) -> Option<Tag> {
        match self {
            Builder::Root(result) => result,
            Builder::Compound { compound, .. } => Some(Tag::Compound(compound)),
            Builder::List(list) => Some(Tag::List(list)),
        }
    }
}

/// `CollectToTag`: rebuilds the visited tag (everything is entered).
pub struct CollectToTag {
    /// `containerStack`; index 0 is the root builder, the last element the innermost container.
    stack: Vec<Builder>,
}

impl Default for CollectToTag {
    fn default() -> Self {
        Self::new()
    }
}

impl CollectToTag {
    pub fn new() -> Self {
        Self { stack: vec![Builder::Root(None)] }
    }

    /// `getResult()`.
    pub fn get_result(&self) -> Option<&Tag> {
        match self.stack.first() {
            Some(Builder::Root(result)) => result.as_ref(),
            _ => None,
        }
    }

    /// `getResult()`, consuming the visitor.
    pub fn into_result(mut self) -> Option<Tag> {
        if self.stack.is_empty() {
            return None;
        }
        self.stack.swap_remove(0).build()
    }

    /// `depth()`: number of containers currently entered.
    pub fn depth(&self) -> i32 {
        self.stack.len() as i32 - 1
    }

    fn append_entry(&mut self, tag: Tag) {
        self.stack.last_mut().expect("CollectToTag: no open container").accept_value(tag);
    }

    fn enter_container_if_needed(&mut self, type_id: u8) {
        match type_id {
            TAG_LIST => self.stack.push(Builder::List(ListTag::new())),
            TAG_COMPOUND => self.stack.push(Builder::Compound { compound: CompoundTag::new(), last_id: String::new() }),
            _ => {}
        }
    }
}

/// Forwards the `StreamTagVisitor` methods that `CollectFields` / `SkipFields` inherit unchanged
/// from `CollectToTag`.
macro_rules! delegate_values {
    ($inner:ident) => {
        fn visit_end(&mut self) -> ValueResult {
            self.$inner.visit_end()
        }
        fn visit_string(&mut self, value: &str) -> ValueResult {
            self.$inner.visit_string(value)
        }
        fn visit_byte(&mut self, value: i8) -> ValueResult {
            self.$inner.visit_byte(value)
        }
        fn visit_short(&mut self, value: i16) -> ValueResult {
            self.$inner.visit_short(value)
        }
        fn visit_int(&mut self, value: i32) -> ValueResult {
            self.$inner.visit_int(value)
        }
        fn visit_long(&mut self, value: i64) -> ValueResult {
            self.$inner.visit_long(value)
        }
        fn visit_float(&mut self, value: f32) -> ValueResult {
            self.$inner.visit_float(value)
        }
        fn visit_double(&mut self, value: f64) -> ValueResult {
            self.$inner.visit_double(value)
        }
        fn visit_byte_array(&mut self, value: &[i8]) -> ValueResult {
            self.$inner.visit_byte_array(value)
        }
        fn visit_int_array(&mut self, value: &[i32]) -> ValueResult {
            self.$inner.visit_int_array(value)
        }
        fn visit_long_array(&mut self, value: &[i64]) -> ValueResult {
            self.$inner.visit_long_array(value)
        }
        fn visit_list(&mut self, element_type_id: u8, size: i32) -> ValueResult {
            self.$inner.visit_list(element_type_id, size)
        }
        fn visit_element(&mut self, type_id: u8, index: i32) -> EntryResult {
            self.$inner.visit_element(type_id, index)
        }
    };
}

impl StreamTagVisitor for CollectToTag {
    fn visit_end(&mut self) -> ValueResult {
        self.append_entry(Tag::End);
        ValueResult::Continue
    }
    fn visit_string(&mut self, value: &str) -> ValueResult {
        self.append_entry(Tag::String(value.to_owned()));
        ValueResult::Continue
    }
    fn visit_byte(&mut self, value: i8) -> ValueResult {
        self.append_entry(Tag::Byte(value));
        ValueResult::Continue
    }
    fn visit_short(&mut self, value: i16) -> ValueResult {
        self.append_entry(Tag::Short(value));
        ValueResult::Continue
    }
    fn visit_int(&mut self, value: i32) -> ValueResult {
        self.append_entry(Tag::Int(value));
        ValueResult::Continue
    }
    fn visit_long(&mut self, value: i64) -> ValueResult {
        self.append_entry(Tag::Long(value));
        ValueResult::Continue
    }
    fn visit_float(&mut self, value: f32) -> ValueResult {
        self.append_entry(Tag::float(value));
        ValueResult::Continue
    }
    fn visit_double(&mut self, value: f64) -> ValueResult {
        self.append_entry(Tag::double(value));
        ValueResult::Continue
    }
    fn visit_byte_array(&mut self, value: &[i8]) -> ValueResult {
        self.append_entry(Tag::ByteArray(value.to_vec()));
        ValueResult::Continue
    }
    fn visit_int_array(&mut self, value: &[i32]) -> ValueResult {
        self.append_entry(Tag::IntArray(value.to_vec()));
        ValueResult::Continue
    }
    fn visit_long_array(&mut self, value: &[i64]) -> ValueResult {
        self.append_entry(Tag::LongArray(value.to_vec()));
        ValueResult::Continue
    }
    fn visit_list(&mut self, _: u8, _: i32) -> ValueResult {
        ValueResult::Continue
    }
    fn visit_element(&mut self, type_id: u8, _: i32) -> EntryResult {
        self.enter_container_if_needed(type_id);
        EntryResult::Enter
    }
    fn visit_entry(&mut self, _: u8) -> EntryResult {
        EntryResult::Enter
    }
    fn visit_entry_named(&mut self, type_id: u8, id: &str) -> EntryResult {
        self.stack.last_mut().expect("CollectToTag: no open container").accept_key(id);
        self.enter_container_if_needed(type_id);
        EntryResult::Enter
    }
    fn visit_container_end(&mut self) -> ValueResult {
        let container = self.stack.pop().expect("CollectToTag: no open container");
        if let Some(tag) = container.build() {
            self.append_entry(tag);
        }
        ValueResult::Continue
    }
    fn visit_root_entry(&mut self, type_id: u8) -> ValueResult {
        self.enter_container_if_needed(type_id);
        ValueResult::Continue
    }
}

// ------------------------------------------------------ FieldSelector / Tree

/// `FieldSelector`: a field `name` of tag type `type_id` below the compounds named by `path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSelector {
    pub path: Vec<String>,
    pub type_id: u8,
    pub name: String,
}

impl FieldSelector {
    /// `FieldSelector(type, name)`: a top-level field.
    pub fn new(type_id: u8, name: impl Into<String>) -> Self {
        Self { path: Vec::new(), type_id, name: name.into() }
    }

    /// `FieldSelector(parent, type, name)`.
    pub fn in_parent(parent: impl Into<String>, type_id: u8, name: impl Into<String>) -> Self {
        Self { path: vec![parent.into()], type_id, name: name.into() }
    }

    /// `FieldSelector(grandparent, parent, type, name)`.
    pub fn in_grandparent(grandparent: impl Into<String>, parent: impl Into<String>, type_id: u8, name: impl Into<String>) -> Self {
        Self { path: vec![grandparent.into(), parent.into()], type_id, name: name.into() }
    }
}

/// `FieldTree`: wanted fields per compound level (`depth` 1 = the root compound).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldTree {
    pub depth: i32,
    pub selected_fields: HashMap<String, u8>,
    pub fields_to_recurse: HashMap<String, FieldTree>,
}

impl FieldTree {
    fn new(depth: i32) -> Self {
        Self { depth, selected_fields: HashMap::new(), fields_to_recurse: HashMap::new() }
    }

    pub fn create_root() -> Self {
        Self::new(1)
    }

    pub fn add_entry(&mut self, field: &FieldSelector) {
        let depth = self.depth;
        if depth as usize <= field.path.len() {
            self.fields_to_recurse
                .entry(field.path[depth as usize - 1].clone())
                .or_insert_with(|| FieldTree::new(depth + 1))
                .add_entry(field);
        } else {
            self.selected_fields.insert(field.name.clone(), field.type_id);
        }
    }

    pub fn is_selected(&self, type_id: u8, id: &str) -> bool {
        self.selected_fields.get(id) == Some(&type_id)
    }
}

/// The `Deque<FieldTree>` of `CollectFields` / `SkipFields`. Vanilla pushes a child frame of the
/// current top, so the stack is always a path down from the root tree; frames are shared, not
/// copied, so a repeated key sees the selections already consumed.
struct FrameStack {
    root: FieldTree,
    /// Keys leading from `root` to the current top frame.
    path: Vec<String>,
    /// The root frame was popped (`ArrayDeque.element()` would throw on the empty deque).
    popped: bool,
}

impl FrameStack {
    fn new(wanted: &[FieldSelector]) -> Self {
        let mut root = FieldTree::create_root();
        for field in wanted {
            root.add_entry(field);
        }
        Self { root, path: Vec::new(), popped: false }
    }

    /// `stack.element()`.
    fn element(&mut self) -> &mut FieldTree {
        assert!(!self.popped, "NoSuchElementException: field frame stack is empty");
        let mut frame = &mut self.root;
        for key in &self.path {
            frame = frame.fields_to_recurse.get_mut(key).expect("frame path follows fields_to_recurse");
        }
        frame
    }

    /// `stack.push(currentFrame.fieldsToRecurse().get(key))`; `key` must exist there.
    fn push(&mut self, key: &str) {
        self.path.push(key.to_owned());
    }

    fn pop(&mut self) {
        if self.path.pop().is_none() {
            self.popped = true;
        }
    }
}

// ------------------------------------------------------------ CollectFields

/// `CollectFields`: collects only the wanted fields (and their parent compounds) of a compound root.
pub struct CollectFields {
    inner: CollectToTag,
    fields_to_get_count: i32,
    wanted_types: Vec<u8>,
    stack: FrameStack,
}

impl CollectFields {
    pub fn new(wanted_fields: &[FieldSelector]) -> Self {
        let mut wanted_types = Vec::new();
        for field in wanted_fields {
            if !wanted_types.contains(&field.type_id) {
                wanted_types.push(field.type_id);
            }
        }
        if !wanted_types.contains(&TAG_COMPOUND) {
            wanted_types.push(TAG_COMPOUND);
        }
        Self { inner: CollectToTag::new(), fields_to_get_count: wanted_fields.len() as i32, wanted_types, stack: FrameStack::new(wanted_fields) }
    }

    /// `getResult()`.
    pub fn get_result(&self) -> Option<&Tag> {
        self.inner.get_result()
    }

    /// `getResult()`, consuming the visitor.
    pub fn into_result(self) -> Option<Tag> {
        self.inner.into_result()
    }

    /// `getMissingFieldCount()`: wanted fields not found (yet).
    pub fn get_missing_field_count(&self) -> i32 {
        self.fields_to_get_count
    }
}

impl StreamTagVisitor for CollectFields {
    delegate_values!(inner);

    fn visit_root_entry(&mut self, type_id: u8) -> ValueResult {
        if type_id != TAG_COMPOUND {
            ValueResult::Halt
        } else {
            self.inner.visit_root_entry(type_id)
        }
    }

    fn visit_entry(&mut self, type_id: u8) -> EntryResult {
        let frame_depth = self.stack.element().depth;
        if self.inner.depth() > frame_depth {
            self.inner.visit_entry(type_id)
        } else if self.fields_to_get_count <= 0 {
            EntryResult::Break
        } else if !self.wanted_types.contains(&type_id) {
            EntryResult::Skip
        } else {
            self.inner.visit_entry(type_id)
        }
    }

    fn visit_entry_named(&mut self, type_id: u8, id: &str) -> EntryResult {
        let depth = self.inner.depth();
        let frame = self.stack.element();
        if depth > frame.depth {
            return self.inner.visit_entry_named(type_id, id);
        }
        if frame.is_selected(type_id, id) {
            frame.selected_fields.remove(id);
            self.fields_to_get_count -= 1;
            return self.inner.visit_entry_named(type_id, id);
        }
        if type_id == TAG_COMPOUND && frame.fields_to_recurse.contains_key(id) {
            self.stack.push(id);
            return self.inner.visit_entry_named(type_id, id);
        }
        EntryResult::Skip
    }

    fn visit_container_end(&mut self) -> ValueResult {
        if self.inner.depth() == self.stack.element().depth {
            self.stack.pop();
        }
        self.inner.visit_container_end()
    }
}

// --------------------------------------------------------------- SkipFields

/// `SkipFields`: collects everything except the given fields.
pub struct SkipFields {
    inner: CollectToTag,
    stack: FrameStack,
}

impl SkipFields {
    pub fn new(unwanted_fields: &[FieldSelector]) -> Self {
        Self { inner: CollectToTag::new(), stack: FrameStack::new(unwanted_fields) }
    }

    /// `getResult()`.
    pub fn get_result(&self) -> Option<&Tag> {
        self.inner.get_result()
    }

    /// `getResult()`, consuming the visitor.
    pub fn into_result(self) -> Option<Tag> {
        self.inner.into_result()
    }
}

impl StreamTagVisitor for SkipFields {
    delegate_values!(inner);

    fn visit_root_entry(&mut self, type_id: u8) -> ValueResult {
        self.inner.visit_root_entry(type_id)
    }

    fn visit_entry(&mut self, type_id: u8) -> EntryResult {
        self.inner.visit_entry(type_id)
    }

    fn visit_entry_named(&mut self, type_id: u8, id: &str) -> EntryResult {
        let frame = self.stack.element();
        if frame.is_selected(type_id, id) {
            return EntryResult::Skip;
        }
        if type_id == TAG_COMPOUND && frame.fields_to_recurse.contains_key(id) {
            self.stack.push(id);
        }
        self.inner.visit_entry_named(type_id, id)
    }

    fn visit_container_end(&mut self) -> ValueResult {
        if self.inner.depth() == self.stack.element().depth {
            self.stack.pop();
        }
        self.inner.visit_container_end()
    }
}
