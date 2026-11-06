//! The value tree all three vanilla `DynamicOps` (NBT, JSON, hash) are run over, plus Java `Number` semantics.
//!
//! One tree type keeps codecs object-safe; the *kind* of ops (see [`crate::Ops`]) decides which constructors and
//! accessors are legal and how the tree is finally rendered (`Tag`, JSON text, CRC32C hash).

use mc_nbt::{CompoundTag, ListTag, Tag};
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Dyn {
    /// NBT `EndTag` / JSON `null` / the hash ops' `empty()`.
    Empty,
    /// JSON / hash booleans (NBT booleans are bytes).
    Bool(bool),
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    /// Gson `LazilyParsedNumber`: a number kept as its source text (JSON input only).
    JsonNum(String),
    Str(String),
    List(Vec<Dyn>),
    /// Ordered entries (NBT compound / JSON object / hash map). Keys are strings except for hash maps.
    Map(Vec<(Dyn, Dyn)>),
    Bytes(Vec<i8>),
    Ints(Vec<i32>),
    Longs(Vec<i64>),
}

impl Dyn {
    pub fn str(s: impl Into<String>) -> Dyn {
        Dyn::Str(s.into())
    }
    pub fn is_empty_value(&self) -> bool {
        matches!(self, Dyn::Empty)
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Dyn::Str(s) => Some(s),
            _ => None,
        }
    }
    /// String-keyed lookup in a `Map` (no JSON-null handling; see `MapLike::get`).
    pub fn map_get(&self, key: &str) -> Option<&Dyn> {
        match self {
            Dyn::Map(m) => m
                .iter()
                .find(|(k, _)| k.as_str() == Some(key))
                .map(|(_, v)| v),
            _ => None,
        }
    }
    /// `CompoundTag.put` / `JsonObject.add`: replace in place or append.
    pub fn map_put(entries: &mut Vec<(Dyn, Dyn)>, key: Dyn, value: Dyn) {
        if let Some(e) = entries.iter_mut().find(|(k, _)| *k == key) {
            e.1 = value;
        } else {
            entries.push((key, value));
        }
    }
    pub fn map_of(entries: impl IntoIterator<Item = (&'static str, Dyn)>) -> Dyn {
        Dyn::Map(entries.into_iter().map(|(k, v)| (Dyn::str(k), v)).collect())
    }
}

/// A `java.lang.Number` as produced by `DynamicOps.getNumberValue`.
#[derive(Clone, Debug, PartialEq)]
pub enum JNum {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    /// `LazilyParsedNumber`.
    Lazy(String),
}

/// `BigDecimal(text)` truncated toward zero to an integer (None when `text` is not a number); low 64 bits like
/// `BigInteger.longValue` for huge values is approximated by saturation-free wrapping through i128 where exact.
fn lazy_trunc(text: &str) -> Option<i128> {
    if let Ok(v) = text.parse::<i128>() {
        return Some(v);
    }
    let d: f64 = text.parse().ok()?;
    if !d.is_finite() {
        return None;
    }
    let t = d.trunc();
    if t.abs() < 1.0e38 {
        Some(t as i128)
    } else {
        Some(0)
    }
}

impl JNum {
    /// `Number.intValue()`.
    pub fn int_value(&self) -> i32 {
        match self {
            JNum::Byte(v) => *v as i32,
            JNum::Short(v) => *v as i32,
            JNum::Int(v) => *v,
            JNum::Long(v) => *v as i32,
            JNum::Float(v) => *v as i32,
            JNum::Double(v) => *v as i32,
            JNum::Lazy(t) => lazy_trunc(t).unwrap_or(0) as i32,
        }
    }
    /// `Number.longValue()`.
    pub fn long_value(&self) -> i64 {
        match self {
            JNum::Byte(v) => *v as i64,
            JNum::Short(v) => *v as i64,
            JNum::Int(v) => *v as i64,
            JNum::Long(v) => *v,
            JNum::Float(v) => *v as i64,
            JNum::Double(v) => *v as i64,
            JNum::Lazy(t) => lazy_trunc(t).unwrap_or(0) as i64,
        }
    }
    /// `Number.byteValue()`: `(byte) intValue()` for floating point and lazy numbers (Java narrows via int).
    pub fn byte_value(&self) -> i8 {
        match self {
            JNum::Byte(v) => *v,
            JNum::Long(v) => *v as i8,
            other => other.int_value() as i8,
        }
    }
    pub fn short_value(&self) -> i16 {
        match self {
            JNum::Short(v) => *v,
            JNum::Long(v) => *v as i16,
            other => other.int_value() as i16,
        }
    }
    pub fn float_value(&self) -> f32 {
        match self {
            JNum::Byte(v) => *v as f32,
            JNum::Short(v) => *v as f32,
            JNum::Int(v) => *v as f32,
            JNum::Long(v) => *v as f32,
            JNum::Float(v) => *v,
            JNum::Double(v) => *v as f32,
            JNum::Lazy(t) => t.parse::<f32>().unwrap_or(0.0),
        }
    }
    pub fn double_value(&self) -> f64 {
        match self {
            JNum::Byte(v) => *v as f64,
            JNum::Short(v) => *v as f64,
            JNum::Int(v) => *v as f64,
            JNum::Long(v) => *v as f64,
            JNum::Float(v) => *v as f64,
            JNum::Double(v) => *v,
            JNum::Lazy(t) => t.parse::<f64>().unwrap_or(0.0),
        }
    }
    pub fn to_dyn(&self) -> Dyn {
        match self {
            JNum::Byte(v) => Dyn::Byte(*v),
            JNum::Short(v) => Dyn::Short(*v),
            JNum::Int(v) => Dyn::Int(*v),
            JNum::Long(v) => Dyn::Long(*v),
            JNum::Float(v) => Dyn::Float(*v),
            JNum::Double(v) => Dyn::Double(*v),
            JNum::Lazy(t) => Dyn::JsonNum(t.clone()),
        }
    }
}

// ---- NBT <-> tree -------------------------------------------------------------------------------------------

impl From<&Tag> for Dyn {
    fn from(t: &Tag) -> Dyn {
        match t {
            Tag::End => Dyn::Empty,
            Tag::Byte(v) => Dyn::Byte(*v),
            Tag::Short(v) => Dyn::Short(*v),
            Tag::Int(v) => Dyn::Int(*v),
            Tag::Long(v) => Dyn::Long(*v),
            Tag::Float(v) => Dyn::Float(*v),
            Tag::Double(v) => Dyn::Double(*v),
            Tag::ByteArray(v) => Dyn::Bytes(v.clone()),
            Tag::String(s) => Dyn::Str(s.clone()),
            Tag::List(l) => Dyn::List(l.iter().map(Dyn::from).collect()),
            // Java `HashMap` iteration order, which is what vanilla codecs observe
            Tag::Compound(c) => Dyn::Map(
                c.iter_java_order()
                    .into_iter()
                    .map(|(k, v)| (Dyn::Str(k.clone()), Dyn::from(v)))
                    .collect(),
            ),
            Tag::IntArray(v) => Dyn::Ints(v.clone()),
            Tag::LongArray(v) => Dyn::Longs(v.clone()),
        }
    }
}

impl Dyn {
    /// Tree built by NBT ops -> `Tag`. Fails on values that cannot exist in NBT (non-string map keys).
    pub fn to_tag(&self) -> Result<Tag, String> {
        Ok(match self {
            Dyn::Empty => Tag::End,
            Dyn::Bool(b) => Tag::from_bool(*b),
            Dyn::Byte(v) => Tag::Byte(*v),
            Dyn::Short(v) => Tag::Short(*v),
            Dyn::Int(v) => Tag::Int(*v),
            Dyn::Long(v) => Tag::Long(*v),
            Dyn::Float(v) => Tag::float(*v),
            Dyn::Double(v) => Tag::double(*v),
            Dyn::JsonNum(t) => return Err(format!("JSON number {t} is not an NBT value")),
            Dyn::Str(s) => Tag::String(s.clone()),
            Dyn::List(l) => Tag::List(l.iter().map(Dyn::to_tag).collect::<Result<ListTag, _>>()?),
            Dyn::Map(m) => {
                let mut c = CompoundTag::new();
                for (k, v) in m {
                    match k {
                        Dyn::Str(s) => {
                            c.put(s.clone(), v.to_tag()?);
                        }
                        other => {
                            return Err(format!("Cannot create map with non-string key: {other}"));
                        }
                    }
                }
                Tag::Compound(c)
            }
            Dyn::Bytes(v) => Tag::ByteArray(v.clone()),
            Dyn::Ints(v) => Tag::IntArray(v.clone()),
            Dyn::Longs(v) => Tag::LongArray(v.clone()),
        })
    }
}

// ---- JSON text ----------------------------------------------------------------------------------------------

impl Dyn {
    /// Parse strict JSON into a tree: numbers keep their text (`JsonNum`), `null` is `Empty`, duplicate object keys
    /// replace the earlier value in place (Gson `JsonObject.add`).
    pub fn from_json_str(text: &str) -> Result<Dyn, String> {
        let mut p = JsonParser {
            s: text.as_bytes(),
            i: 0,
            depth: 0,
        };
        p.ws();
        let v = p.value()?;
        p.ws();
        if p.i != p.s.len() {
            return Err(format!("Unexpected trailing data at offset {}", p.i));
        }
        Ok(v)
    }

    /// Compact JSON text like `JsonElement.toString()` (no HTML escaping). Floats print like `Float.toString`.
    pub fn to_json_string(&self) -> String {
        let mut out = String::new();
        self.write_json(&mut out);
        out
    }

    fn write_json(&self, out: &mut String) {
        match self {
            Dyn::Empty => out.push_str("null"),
            Dyn::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Dyn::Byte(v) => out.push_str(&v.to_string()),
            Dyn::Short(v) => out.push_str(&v.to_string()),
            Dyn::Int(v) => out.push_str(&v.to_string()),
            Dyn::Long(v) => out.push_str(&v.to_string()),
            Dyn::Float(v) => out.push_str(&mc_core::math::java::java_float_to_string(*v)),
            Dyn::Double(v) => out.push_str(&mc_core::math::java::java_double_to_string(*v)),
            Dyn::JsonNum(t) => out.push_str(t),
            Dyn::Str(s) => write_json_string(out, s),
            Dyn::List(l) => {
                out.push('[');
                for (i, e) in l.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    e.write_json(out);
                }
                out.push(']');
            }
            Dyn::Map(m) => {
                out.push('{');
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    match k {
                        Dyn::Str(s) => write_json_string(out, s),
                        other => write_json_string(out, &other.to_json_string()),
                    }
                    out.push(':');
                    v.write_json(out);
                }
                out.push('}');
            }
            Dyn::Bytes(v) => write_nums(out, v.iter().map(|x| x.to_string())),
            Dyn::Ints(v) => write_nums(out, v.iter().map(|x| x.to_string())),
            Dyn::Longs(v) => write_nums(out, v.iter().map(|x| x.to_string())),
        }
    }
}

fn write_nums(out: &mut String, it: impl Iterator<Item = String>) {
    out.push('[');
    for (i, n) in it.enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&n);
    }
    out.push(']');
}

fn write_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' | '\u{2029}' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

struct JsonParser<'a> {
    s: &'a [u8],
    i: usize,
    depth: usize,
}

impl JsonParser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }
    fn err<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("{what} at offset {}", self.i))
    }
    fn value(&mut self) -> Result<Dyn, String> {
        if self.depth > 512 {
            return self.err("Nesting too deep");
        }
        match self.s.get(self.i) {
            None => self.err("Unexpected end of input"),
            Some(b'{') => {
                self.i += 1;
                self.depth += 1;
                let mut m: Vec<(Dyn, Dyn)> = Vec::new();
                self.ws();
                if self.s.get(self.i) == Some(&b'}') {
                    self.i += 1;
                } else {
                    loop {
                        self.ws();
                        if self.s.get(self.i) != Some(&b'"') {
                            return self.err("Expected name");
                        }
                        let k = self.string()?;
                        self.ws();
                        if self.s.get(self.i) != Some(&b':') {
                            return self.err("Expected ':'");
                        }
                        self.i += 1;
                        self.ws();
                        let v = self.value()?;
                        Dyn::map_put(&mut m, Dyn::Str(k), v);
                        self.ws();
                        match self.s.get(self.i) {
                            Some(b',') => self.i += 1,
                            Some(b'}') => {
                                self.i += 1;
                                break;
                            }
                            _ => return self.err("Expected ',' or '}'"),
                        }
                    }
                }
                self.depth -= 1;
                Ok(Dyn::Map(m))
            }
            Some(b'[') => {
                self.i += 1;
                self.depth += 1;
                let mut l = Vec::new();
                self.ws();
                if self.s.get(self.i) == Some(&b']') {
                    self.i += 1;
                } else {
                    loop {
                        self.ws();
                        l.push(self.value()?);
                        self.ws();
                        match self.s.get(self.i) {
                            Some(b',') => self.i += 1,
                            Some(b']') => {
                                self.i += 1;
                                break;
                            }
                            _ => return self.err("Expected ',' or ']'"),
                        }
                    }
                }
                self.depth -= 1;
                Ok(Dyn::List(l))
            }
            Some(b'"') => Ok(Dyn::Str(self.string()?)),
            Some(b't') if self.s[self.i..].starts_with(b"true") => {
                self.i += 4;
                Ok(Dyn::Bool(true))
            }
            Some(b'f') if self.s[self.i..].starts_with(b"false") => {
                self.i += 5;
                Ok(Dyn::Bool(false))
            }
            Some(b'n') if self.s[self.i..].starts_with(b"null") => {
                self.i += 4;
                Ok(Dyn::Empty)
            }
            Some(b'-' | b'0'..=b'9') => {
                let start = self.i;
                self.i += 1;
                while self.i < self.s.len()
                    && matches!(
                        self.s[self.i],
                        b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-'
                    )
                {
                    self.i += 1;
                }
                let t = std::str::from_utf8(&self.s[start..self.i]).unwrap();
                // RFC 8259 number grammar
                let b = t.as_bytes();
                let mut j = 0;
                if b[j] == b'-' {
                    j += 1;
                }
                let digits = |j: &mut usize| {
                    let s0 = *j;
                    while *j < b.len() && b[*j].is_ascii_digit() {
                        *j += 1;
                    }
                    *j > s0
                };
                if j < b.len() && b[j] == b'0' {
                    j += 1;
                } else if !digits(&mut j) {
                    return self.err("Malformed number");
                }
                if j < b.len() && b[j] == b'.' {
                    j += 1;
                    if !digits(&mut j) {
                        return self.err("Malformed number");
                    }
                }
                if j < b.len() && (b[j] == b'e' || b[j] == b'E') {
                    j += 1;
                    if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
                        j += 1;
                    }
                    if !digits(&mut j) {
                        return self.err("Malformed number");
                    }
                }
                if j != b.len() {
                    return self.err("Malformed number");
                }
                Ok(Dyn::JsonNum(t.to_string()))
            }
            _ => self.err("Unexpected character"),
        }
    }
    fn string(&mut self) -> Result<String, String> {
        self.i += 1;
        let mut units: Vec<u16> = Vec::new();
        loop {
            let Some(&c) = self.s.get(self.i) else {
                return self.err("Unterminated string");
            };
            match c {
                b'"' => {
                    self.i += 1;
                    break;
                }
                b'\\' => {
                    let Some(&e) = self.s.get(self.i + 1) else {
                        return self.err("Bad escape");
                    };
                    self.i += 2;
                    let u = match e {
                        b'"' => b'"' as u16,
                        b'\\' => b'\\' as u16,
                        b'/' => b'/' as u16,
                        b'b' => 8,
                        b'f' => 12,
                        b'n' => 10,
                        b'r' => 13,
                        b't' => 9,
                        b'u' => {
                            let h = self
                                .s
                                .get(self.i..self.i + 4)
                                .and_then(|h| std::str::from_utf8(h).ok());
                            let Some(v) = h.and_then(|h| u16::from_str_radix(h, 16).ok()) else {
                                return self.err("Bad \\u escape");
                            };
                            self.i += 4;
                            v
                        }
                        _ => return self.err("Invalid escape sequence"),
                    };
                    units.push(u);
                }
                c if c < 0x20 => return self.err("Control character in string"),
                _ => {
                    let rest = std::str::from_utf8(&self.s[self.i..])
                        .map_err(|_| format!("Invalid UTF-8 at offset {}", self.i))?;
                    let ch = rest.chars().next().unwrap();
                    let mut buf = [0u16; 2];
                    units.extend_from_slice(ch.encode_utf16(&mut buf));
                    self.i += ch.len_utf8();
                }
            }
        }
        Ok(String::from_utf16_lossy(&units))
    }
}

impl fmt::Display for Dyn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_json_string())
    }
}
