//! Binary NBT IO: `Tag.write`, `TagType.load`, and Java `DataInput`/`DataOutput` primitives
//! (big-endian, modified UTF-8).

use crate::accounter::NbtAccounter;
use crate::error::{NbtError, Result};
use crate::list::is_wrapper;
use crate::tag::*;
use crate::{CompoundTag, ListTag};

// ---------------------------------------------------------------- DataInput

pub(crate) fn take<'a>(input: &mut &'a [u8], n: usize) -> Result<&'a [u8]> {
    if input.len() < n {
        // DataInputStream consumes what is left before throwing EOF.
        *input = &input[input.len()..];
        return Err(NbtError::Eof);
    }
    let (head, tail) = input.split_at(n);
    *input = tail;
    Ok(head)
}

pub(crate) fn read_u8(input: &mut &[u8]) -> Result<u8> {
    Ok(take(input, 1)?[0])
}
pub(crate) fn read_i8(input: &mut &[u8]) -> Result<i8> {
    Ok(read_u8(input)? as i8)
}
pub(crate) fn read_u16(input: &mut &[u8]) -> Result<u16> {
    Ok(u16::from_be_bytes(take(input, 2)?.try_into().unwrap()))
}
pub(crate) fn read_i16(input: &mut &[u8]) -> Result<i16> {
    Ok(read_u16(input)? as i16)
}
pub(crate) fn read_i32(input: &mut &[u8]) -> Result<i32> {
    Ok(i32::from_be_bytes(take(input, 4)?.try_into().unwrap()))
}
pub(crate) fn read_i64(input: &mut &[u8]) -> Result<i64> {
    Ok(i64::from_be_bytes(take(input, 8)?.try_into().unwrap()))
}
pub(crate) fn read_f32(input: &mut &[u8]) -> Result<f32> {
    Ok(f32::from_bits(read_i32(input)? as u32))
}
pub(crate) fn read_f64(input: &mut &[u8]) -> Result<f64> {
    Ok(f64::from_bits(read_i64(input)? as u64))
}

/// `DataInputStream.readUTF`. Lone surrogates (representable in Java strings) become U+FFFD.
pub(crate) fn read_utf(input: &mut &[u8]) -> Result<String> {
    let utflen = read_u16(input)? as usize;
    let bytes = take(input, utflen)?;
    if bytes.is_ascii() {
        // Safe fast path; embedded NUL bytes are valid single-byte chars as well.
        return Ok(String::from_utf8_lossy(bytes).into_owned());
    }
    let mut units: Vec<u16> = Vec::with_capacity(utflen);
    let mut count = 0usize;
    while count < utflen {
        let c = bytes[count] as u32;
        match c >> 4 {
            0..=7 => {
                count += 1;
                units.push(c as u16);
            }
            12 | 13 => {
                count += 2;
                if count > utflen {
                    return Err(NbtError::MalformedUtf("malformed input: partial character at end".into()));
                }
                let c2 = bytes[count - 1] as u32;
                if c2 & 0xC0 != 0x80 {
                    return Err(NbtError::MalformedUtf(format!("malformed input around byte {count}")));
                }
                units.push((((c & 0x1F) << 6) | (c2 & 0x3F)) as u16);
            }
            14 => {
                count += 3;
                if count > utflen {
                    return Err(NbtError::MalformedUtf("malformed input: partial character at end".into()));
                }
                let c2 = bytes[count - 2] as u32;
                let c3 = bytes[count - 1] as u32;
                if c2 & 0xC0 != 0x80 || c3 & 0xC0 != 0x80 {
                    return Err(NbtError::MalformedUtf(format!("malformed input around byte {}", count - 1)));
                }
                units.push((((c & 0x0F) << 12) | ((c2 & 0x3F) << 6) | (c3 & 0x3F)) as u16);
            }
            _ => return Err(NbtError::MalformedUtf(format!("malformed input around byte {count}"))),
        }
    }
    Ok(String::from_utf16_lossy(&units))
}

pub(crate) fn skip_bytes(input: &mut &[u8], n: usize) {
    // DataInput.skipBytes never throws on a short stream; it just skips what is there.
    let n = n.min(input.len());
    *input = &input[n..];
}

// --------------------------------------------------------------- DataOutput

/// `DataOutputStream.writeUTF`. With `fallback`, an over-long string is replaced by `""`
/// (`NbtIo.StringFallbackDataOutput`).
pub(crate) fn write_utf(out: &mut Vec<u8>, s: &str, fallback: bool) -> Result<()> {
    let mut utflen = 0usize;
    for u in s.encode_utf16() {
        utflen += match u {
            0x0001..=0x007F => 1,
            0x0800..=0xFFFF => 3,
            _ => 2,
        };
    }
    if utflen > 65535 {
        if fallback {
            out.extend_from_slice(&[0, 0]);
            return Ok(());
        }
        let units: Vec<u16> = s.encode_utf16().collect();
        let head = String::from_utf16_lossy(&units[..8]);
        let tail = String::from_utf16_lossy(&units[units.len() - 8..]);
        return Err(NbtError::MalformedUtf(format!("encoded string ({head}...{tail}) too long: {utflen} bytes")));
    }
    out.extend_from_slice(&(utflen as u16).to_be_bytes());
    if s.is_ascii() && !s.contains('\0') {
        out.extend_from_slice(s.as_bytes());
        return Ok(());
    }
    for u in s.encode_utf16() {
        match u {
            0x0001..=0x007F => out.push(u as u8),
            0x0800..=0xFFFF => {
                out.push(0xE0 | (u >> 12) as u8);
                out.push(0x80 | ((u >> 6) & 0x3F) as u8);
                out.push(0x80 | (u & 0x3F) as u8);
            }
            _ => {
                out.push(0xC0 | ((u >> 6) & 0x1F) as u8);
                out.push(0x80 | (u & 0x3F) as u8);
            }
        }
    }
    Ok(())
}

// -------------------------------------------------------------------- write

impl Tag {
    /// `Tag.write(DataOutput)`: the payload only (no id, no name). Strict: an over-long string is
    /// an error (as with a plain `DataOutput`).
    pub fn write(&self, out: &mut Vec<u8>) -> Result<()> {
        self.write_payload(out, false)
    }

    pub(crate) fn write_payload(&self, out: &mut Vec<u8>, fallback: bool) -> Result<()> {
        match self {
            Tag::End => {}
            Tag::Byte(v) => out.push(*v as u8),
            Tag::Short(v) => out.extend_from_slice(&v.to_be_bytes()),
            Tag::Int(v) => out.extend_from_slice(&v.to_be_bytes()),
            Tag::Long(v) => out.extend_from_slice(&v.to_be_bytes()),
            // DataOutput.writeFloat/writeDouble use floatToIntBits/doubleToLongBits: NaN is canonical.
            Tag::Float(v) => out.extend_from_slice(&(if v.is_nan() { 0x7fc0_0000 } else { v.to_bits() }).to_be_bytes()),
            Tag::Double(v) => out.extend_from_slice(&(if v.is_nan() { 0x7ff8_0000_0000_0000 } else { v.to_bits() }).to_be_bytes()),
            Tag::ByteArray(v) => {
                out.extend_from_slice(&(v.len() as i32).to_be_bytes());
                out.extend(v.iter().map(|b| *b as u8));
            }
            Tag::String(s) => write_utf(out, s, fallback)?,
            Tag::List(l) => l.write_payload(out, fallback)?,
            Tag::Compound(c) => c.write_payload(out, fallback)?,
            Tag::IntArray(v) => {
                out.extend_from_slice(&(v.len() as i32).to_be_bytes());
                for i in v {
                    out.extend_from_slice(&i.to_be_bytes());
                }
            }
            Tag::LongArray(v) => {
                out.extend_from_slice(&(v.len() as i32).to_be_bytes());
                for i in v {
                    out.extend_from_slice(&i.to_be_bytes());
                }
            }
        }
        Ok(())
    }
}

impl CompoundTag {
    pub(crate) fn write_payload(&self, out: &mut Vec<u8>, fallback: bool) -> Result<()> {
        for (key, tag) in self.iter_java_order() {
            out.push(tag.id());
            if tag.id() != 0 {
                write_utf(out, key, fallback)?;
                tag.write_payload(out, fallback)?;
            }
        }
        out.push(0);
        Ok(())
    }
}

impl ListTag {
    pub(crate) fn write_payload(&self, out: &mut Vec<u8>, fallback: bool) -> Result<()> {
        let element_type = self.identify_raw_element_type();
        out.push(element_type);
        out.extend_from_slice(&(self.len() as i32).to_be_bytes());
        for element in self.iter() {
            match element {
                Tag::Compound(c) if element_type == TAG_COMPOUND && !is_wrapper(c) => c.write_payload(out, fallback)?,
                _ if element_type == TAG_COMPOUND => {
                    // wrapElement: {"": element}
                    out.push(element.id());
                    if element.id() != 0 {
                        write_utf(out, "", fallback)?;
                        element.write_payload(out, fallback)?;
                    }
                    out.push(0);
                }
                _ => element.write_payload(out, fallback)?,
            }
        }
        Ok(())
    }
}

// --------------------------------------------------------------------- load

impl Tag {
    /// `TagType.load`: reads the payload of a tag of type `id`.
    pub fn load(id: u8, input: &mut &[u8], acc: &mut NbtAccounter) -> Result<Tag> {
        match id {
            TAG_END => {
                acc.account_bytes(8)?;
                Ok(Tag::End)
            }
            TAG_BYTE => {
                acc.account_bytes(9)?;
                Ok(Tag::Byte(read_i8(input)?))
            }
            TAG_SHORT => {
                acc.account_bytes(10)?;
                Ok(Tag::Short(read_i16(input)?))
            }
            TAG_INT => {
                acc.account_bytes(12)?;
                Ok(Tag::Int(read_i32(input)?))
            }
            TAG_LONG => {
                acc.account_bytes(16)?;
                Ok(Tag::Long(read_i64(input)?))
            }
            TAG_FLOAT => {
                acc.account_bytes(12)?;
                Ok(Tag::float(read_f32(input)?))
            }
            TAG_DOUBLE => {
                acc.account_bytes(16)?;
                Ok(Tag::double(read_f64(input)?))
            }
            TAG_BYTE_ARRAY => {
                acc.account_bytes(24)?;
                let len = read_i32(input)?;
                acc.account_bytes_n(1, len as i64)?;
                let bytes = take(input, len as usize)?;
                Ok(Tag::ByteArray(bytes.iter().map(|b| *b as i8).collect()))
            }
            TAG_STRING => {
                acc.account_bytes(36)?;
                let s = read_utf(input)?;
                acc.account_bytes_n(2, s.encode_utf16().count() as i64)?;
                Ok(Tag::String(s))
            }
            TAG_LIST => {
                acc.push_depth()?;
                let r = load_list(input, acc);
                acc.pop_depth()?;
                r.map(Tag::List)
            }
            TAG_COMPOUND => {
                acc.push_depth()?;
                let r = load_compound(input, acc);
                acc.pop_depth()?;
                r.map(Tag::Compound)
            }
            TAG_INT_ARRAY => {
                acc.account_bytes(24)?;
                let len = read_i32(input)?;
                acc.account_bytes_n(4, len as i64)?;
                let bytes = take(input, len as usize * 4)?;
                Ok(Tag::IntArray(bytes.chunks_exact(4).map(|c| i32::from_be_bytes(c.try_into().unwrap())).collect()))
            }
            TAG_LONG_ARRAY => {
                acc.account_bytes(24)?;
                let len = read_i32(input)?;
                acc.account_bytes_n(8, len as i64)?;
                let bytes = take(input, len as usize * 8)?;
                Ok(Tag::LongArray(bytes.chunks_exact(8).map(|c| i64::from_be_bytes(c.try_into().unwrap())).collect()))
            }
            _ => Err(NbtError::InvalidTagId(id as i8 as i32)),
        }
    }
}

fn read_list_count(input: &mut &[u8]) -> Result<i32> {
    let count = read_i32(input)?;
    if count < 0 {
        return Err(NbtError::Format(format!("ListTag length cannot be negative: {count}")));
    }
    Ok(count)
}

fn load_list(input: &mut &[u8], acc: &mut NbtAccounter) -> Result<ListTag> {
    acc.account_bytes(36)?;
    let type_id = read_u8(input)?;
    let count = read_list_count(input)?;
    if type_id == 0 && count > 0 {
        return Err(NbtError::Format("Missing type on ListTag".into()));
    }
    acc.account_bytes_n(4, count as i64)?;
    // Every element occupies at least one byte, so never trust `count` for allocation.
    let mut list = ListTag::with_capacity((count as usize).min(input.len()));
    for _ in 0..count {
        list.add_and_unwrap(Tag::load(type_id, input, acc)?);
    }
    Ok(list)
}

fn read_key(input: &mut &[u8], acc: &mut NbtAccounter) -> Result<String> {
    let key = read_utf(input)?;
    acc.account_bytes(28)?;
    acc.account_bytes_n(2, key.encode_utf16().count() as i64)?;
    Ok(key)
}

fn load_compound(input: &mut &[u8], acc: &mut NbtAccounter) -> Result<CompoundTag> {
    acc.account_bytes(48)?;
    let mut tag = CompoundTag::new();
    loop {
        let tag_type = read_u8(input)?;
        if tag_type == 0 {
            break;
        }
        let key = read_key(input, acc)?;
        let value = read_named_tag_data(tag_type, &key, input, acc)?;
        if tag.put(key, value).is_none() {
            acc.account_bytes(36)?;
        }
    }
    Ok(tag)
}

/// `CompoundTag.readNamedTagData`: IO failures become a `ReportedNbtException`.
fn read_named_tag_data(tag_type: u8, name: &str, input: &mut &[u8], acc: &mut NbtAccounter) -> Result<Tag> {
    Tag::load(tag_type, input, acc).map_err(|e| {
        if e.is_io() {
            NbtError::Reported { tag_name: Some(name.to_string()), tag_type: Tag::type_name(tag_type), cause: Box::new(e) }
        } else {
            e
        }
    })
}

/// `NbtIo.readTagSafe`.
fn read_tag_safe(input: &mut &[u8], acc: &mut NbtAccounter, type_id: u8) -> Result<Tag> {
    Tag::load(type_id, input, acc).map_err(|e| {
        if e.is_io() {
            NbtError::Reported { tag_name: None, tag_type: (type_id as i8).to_string(), cause: Box::new(e) }
        } else {
            e
        }
    })
}

// ------------------------------------------------------------------- NbtIo

/// `NbtIo.readUnnamedTag`: type id, an (ignored) name, then the payload. `End` for id 0.
pub fn read_unnamed_tag(input: &mut &[u8], acc: &mut NbtAccounter) -> Result<Tag> {
    let type_id = read_u8(input)?;
    if type_id == 0 {
        return Ok(Tag::End);
    }
    let name_len = read_u16(input)? as usize;
    skip_bytes(input, name_len);
    read_tag_safe(input, acc, type_id)
}

/// `NbtIo.read(DataInput, NbtAccounter)`: a named root that must be a compound (files).
pub fn read(input: &mut &[u8], acc: &mut NbtAccounter) -> Result<CompoundTag> {
    match read_unnamed_tag(input, acc)? {
        Tag::Compound(c) => Ok(c),
        _ => Err(NbtError::Io("Root tag must be a named compound tag".into())),
    }
}

/// `NbtIo.readAnyTag`: network format — type id then payload, no name. `End` for id 0.
pub fn read_any_tag(input: &mut &[u8], acc: &mut NbtAccounter) -> Result<Tag> {
    let type_id = read_u8(input)?;
    if type_id == 0 {
        return Ok(Tag::End);
    }
    read_tag_safe(input, acc, type_id)
}

/// `NbtIo.writeAnyTag`: network format.
pub fn write_any_tag(tag: &Tag, out: &mut Vec<u8>) -> Result<()> {
    out.push(tag.id());
    if tag.id() != 0 {
        tag.write_payload(out, false)?;
    }
    Ok(())
}

/// `NbtIo.writeUnnamedTag`: id, empty name, payload (strict strings).
pub fn write_unnamed_tag(tag: &Tag, out: &mut Vec<u8>) -> Result<()> {
    out.push(tag.id());
    if tag.id() != 0 {
        write_utf(out, "", false)?;
        tag.write_payload(out, false)?;
    }
    Ok(())
}

/// `NbtIo.writeUnnamedTagWithFallback`: like `write_unnamed_tag`, but an over-long string is
/// written as `""` instead of failing.
pub fn write_unnamed_tag_with_fallback(tag: &Tag, out: &mut Vec<u8>) {
    out.push(tag.id());
    if tag.id() != 0 {
        out.extend_from_slice(&[0, 0]);
        tag.write_payload(out, true).expect("fallback writes cannot fail");
    }
}

/// `NbtIo.write(CompoundTag, DataOutput)`: named (empty-name) compound root, as in `level.dat`
/// and region chunk payloads.
pub fn write(tag: &CompoundTag, out: &mut Vec<u8>) {
    out.push(TAG_COMPOUND);
    out.extend_from_slice(&[0, 0]);
    tag.write_payload(out, true).expect("fallback writes cannot fail");
}

/// [`write`] into a fresh buffer.
pub fn to_bytes(tag: &CompoundTag) -> Vec<u8> {
    let mut out = Vec::new();
    write(tag, &mut out);
    out
}
