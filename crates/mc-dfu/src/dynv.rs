//! The value tree all three vanilla `DynamicOps` (NBT, JSON, hash) are run over, plus Java `Number` semantics.
//!
//! One tree type keeps codecs object-safe; the *kind* of ops (see [`crate::Ops`]) decides which constructors and
//! accessors are legal and how the tree is finally rendered (`Tag`, JSON text, CRC32C hash).

use mc_core::gson::{JsonElement, JsonObject};
use mc_core::java_string::JavaString;
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
    Str(JavaString),
    List(Vec<Dyn>),
    /// Ordered entries (NBT compound / JSON object / hash map). Keys are strings except for hash maps.
    Map(Vec<(Dyn, Dyn)>),
    Bytes(Vec<i8>),
    Ints(Vec<i32>),
    Longs(Vec<i64>),
}

impl Dyn {
    pub fn str(s: impl Into<JavaString>) -> Dyn {
        Dyn::Str(s.into())
    }
    pub fn is_empty_value(&self) -> bool {
        matches!(self, Dyn::Empty)
    }
    /// The text of a string, each lone surrogate shown as U+FFFD.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Dyn::Str(s) => Some(s.as_lossy_str()),
            _ => None,
        }
    }
    /// The exact string, lone surrogates included.
    pub fn as_java_string(&self) -> Option<&JavaString> {
        match self {
            Dyn::Str(s) => Some(s),
            _ => None,
        }
    }
    /// Whether this is the string `key` (never a string with lone surrogates).
    pub fn is_str(&self, key: &str) -> bool {
        matches!(self, Dyn::Str(s) if *s == *key)
    }
    /// String-keyed lookup in a `Map` (no JSON-null handling; see `MapLike::get`).
    pub fn map_get(&self, key: &str) -> Option<&Dyn> {
        match self {
            Dyn::Map(m) => m.iter().find(|(k, _)| k.is_str(key)).map(|(_, v)| v),
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

/// Throws a `NumberFormatException` out of a codec (a Java exception unwinds through the decoder to whoever catches it).
/// `resume_unwind` skips the panic hook: vanilla prints nothing for an exception that is caught.
pub fn throw_number_format(message: &str) -> ! {
    std::panic::resume_unwind(Box::new(format!(
        "java.lang.NumberFormatException: {message}"
    )))
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
            JNum::Lazy(t) => mc_core::gson::lazily_parsed_int_value(t)
                .unwrap_or_else(|e| throw_number_format(&e)),
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
            JNum::Lazy(t) => mc_core::gson::lazily_parsed_long_value(t)
                .unwrap_or_else(|e| throw_number_format(&e)),
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
                let mut entries = Vec::with_capacity(m.len());
                for (k, v) in m {
                    match k {
                        Dyn::Str(s) => entries.push((s.clone(), v.to_tag()?)),
                        other => {
                            return Err(format!("Cannot create map with non-string key: {other}"));
                        }
                    }
                }
                // a map's entry order is the iteration order of the table it was built in
                Tag::Compound(CompoundTag::from_hash_order(entries))
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
    pub fn to_json_string(&self) -> JavaString {
        let mut out = JavaString::new();
        self.write_json(&mut out);
        out
    }

    fn write_json(&self, out: &mut JavaString) {
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

impl Dyn {
    /// The Gson tree of this value (`JsonOps.convertTo`-like): numbers print like Java prints them (`Float.toString`
    /// for floats), a `NaN` or infinite float is `null` (Gson rejects it), a non-string map key is its JSON text.
    pub fn to_json_element(&self) -> JsonElement {
        let num = |t: String| JsonElement::Number(t);
        let nums = |it: &mut dyn Iterator<Item = String>| JsonElement::Array(it.map(num).collect());
        match self {
            Dyn::Empty => JsonElement::Null,
            Dyn::Bool(b) => JsonElement::Bool(*b),
            Dyn::Byte(v) => num(v.to_string()),
            Dyn::Short(v) => num(v.to_string()),
            Dyn::Int(v) => num(v.to_string()),
            Dyn::Long(v) => num(v.to_string()),
            Dyn::Float(v) if v.is_finite() => num(mc_core::math::java::java_float_to_string(*v)),
            Dyn::Double(v) if v.is_finite() => num(mc_core::math::java::java_double_to_string(*v)),
            Dyn::Float(_) | Dyn::Double(_) => JsonElement::Null,
            Dyn::JsonNum(t) => num(t.clone()),
            Dyn::Str(s) => JsonElement::String(s.clone()),
            Dyn::List(l) => JsonElement::Array(l.iter().map(Dyn::to_json_element).collect()),
            Dyn::Map(m) => JsonElement::Object(JsonObject::from_distinct_members(
                m.iter()
                    .map(|(k, v)| {
                        let key = match k {
                            Dyn::Str(s) => s.clone(),
                            other => other.to_json_string(),
                        };
                        (key, v.to_json_element())
                    })
                    .collect(),
            )),
            Dyn::Bytes(v) => nums(&mut v.iter().map(|x| x.to_string())),
            Dyn::Ints(v) => nums(&mut v.iter().map(|x| x.to_string())),
            Dyn::Longs(v) => nums(&mut v.iter().map(|x| x.to_string())),
        }
    }

    /// The value of a Gson tree: numbers keep their source text (`JsonNum`), `null` is `Empty`.
    pub fn from_json_element(e: &JsonElement) -> Dyn {
        match e {
            JsonElement::Null => Dyn::Empty,
            JsonElement::Bool(b) => Dyn::Bool(*b),
            JsonElement::Number(t) => Dyn::JsonNum(t.clone()),
            JsonElement::String(s) => Dyn::Str(s.clone()),
            JsonElement::Array(a) => Dyn::List(a.iter().map(Dyn::from_json_element).collect()),
            JsonElement::Object(o) => Dyn::Map(
                o.iter()
                    .map(|(k, v)| (Dyn::Str(k.clone()), Dyn::from_json_element(v)))
                    .collect(),
            ),
        }
    }
}

fn write_nums(out: &mut JavaString, it: impl Iterator<Item = String>) {
    out.push('[');
    for (i, n) in it.enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&n);
    }
    out.push(']');
}

fn write_json_string(out: &mut JavaString, s: &JavaString) {
    out.push('"');
    for c in char::decode_utf16(s.encode_utf16()) {
        let c = match c {
            Ok(c) => c,
            Err(e) => {
                out.push_utf16_unit(e.unpaired_surrogate());
                continue;
            }
        };
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
    fn string(&mut self) -> Result<JavaString, String> {
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
        Ok(JavaString::from_utf16_units(&units))
    }
}

impl fmt::Display for Dyn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_json_string().to_utf8_string())
    }
}

#[cfg(test)]
mod display_tests {
    use super::*;

    #[test]
    fn display_prints_a_lone_surrogate_as_a_question_mark() {
        let s = JavaString::from_utf16_units(&[0x61, 0xD800]);
        assert_eq!(Dyn::Str(s).to_string(), "\"a?\"");
    }
}
