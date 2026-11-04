//! The slice of Gson vanilla's user lists, `usercache.json` and `generator-settings` rely on: an insertion-ordered
//! `JsonElement` tree, a reader (strict, or Gson's default "legacy lenient" `fromJson(Reader)` mode) and the
//! `JsonWriter` output formats (compact / pretty-printed with 2-space indent, optional HTML-safe escaping).

use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{0}")]
pub struct JsonError(pub String);

impl JsonError {
    fn new(msg: impl Into<String>) -> JsonError {
        JsonError(msg.into())
    }
}

/// `JsonElement`. Numbers keep their source text (Gson's `LazilyParsedNumber`).
#[derive(Debug, Clone, PartialEq)]
pub enum JsonElement {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<JsonElement>),
    Object(JsonObject),
}

/// `JsonObject`: members in insertion order (`LinkedTreeMap`); re-adding a key keeps its position.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JsonObject {
    members: Vec<(String, JsonElement)>,
}

impl JsonObject {
    pub fn new() -> JsonObject {
        JsonObject::default()
    }

    pub fn add(&mut self, key: &str, value: JsonElement) {
        match self.members.iter_mut().find(|(k, _)| k == key) {
            Some((_, v)) => *v = value,
            None => self.members.push((key.to_owned(), value)),
        }
    }

    pub fn add_property_str(&mut self, key: &str, value: Option<&str>) {
        self.add(key, value.map_or(JsonElement::Null, |v| JsonElement::String(v.to_owned())));
    }

    pub fn add_property_int(&mut self, key: &str, value: i64) {
        self.add(key, JsonElement::Number(value.to_string()));
    }

    pub fn add_property_bool(&mut self, key: &str, value: bool) {
        self.add(key, JsonElement::Bool(value));
    }

    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    pub fn get(&self, key: &str) -> Option<&JsonElement> {
        self.members.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &JsonElement)> {
        self.members.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }
}

impl JsonElement {
    /// `getAsString` (primitives only; `Err` is the exception vanilla would throw).
    pub fn as_string(&self) -> Result<String, JsonError> {
        match self {
            JsonElement::String(s) | JsonElement::Number(s) => Ok(s.clone()),
            JsonElement::Bool(b) => Ok(b.to_string()),
            JsonElement::Array(a) if a.len() == 1 => a[0].as_string(),
            _ => Err(JsonError::new("Not a JSON primitive")),
        }
    }

    /// `getAsInt` (`Integer.parseInt`, then `Long`, then `BigDecimal.intValue` for a number; the string is parsed for
    /// string primitives).
    pub fn as_int(&self) -> Result<i32, JsonError> {
        match self {
            JsonElement::Number(text) | JsonElement::String(text) => {
                if let Ok(v) = text.parse::<i32>() {
                    return Ok(v);
                }
                if let Ok(v) = text.parse::<i64>() {
                    return Ok(v as i32);
                }
                text.parse::<f64>().map(|v| v as i32).map_err(|_| JsonError::new(format!("For input string: \"{text}\"")))
            }
            JsonElement::Array(a) if a.len() == 1 => a[0].as_int(),
            _ => Err(JsonError::new("Not a JSON primitive")),
        }
    }

    /// `getAsBoolean`.
    pub fn as_bool(&self) -> Result<bool, JsonError> {
        match self {
            JsonElement::Bool(b) => Ok(*b),
            JsonElement::Number(s) | JsonElement::String(s) => Ok(s.eq_ignore_ascii_case("true")),
            JsonElement::Array(a) if a.len() == 1 => a[0].as_bool(),
            _ => Err(JsonError::new("Not a JSON primitive")),
        }
    }

    pub fn as_object(&self) -> Option<&JsonObject> {
        match self {
            JsonElement::Object(o) => Some(o),
            _ => None,
        }
    }

    /// `JsonElement.toString()`: compact, no HTML escaping, nulls kept.
    pub fn to_compact_string(&self) -> String {
        let mut out = String::new();
        write_element(&mut out, self, &Format { indent: false, html_safe: false, serialize_nulls: true }, 0);
        out
    }

    /// `new GsonBuilder().create().toJson(element)`: compact with HTML-safe escaping, nulls skipped.
    pub fn to_gson_string(&self) -> String {
        let mut out = String::new();
        write_element(&mut out, self, &Format { indent: false, html_safe: true, serialize_nulls: false }, 0);
        out
    }

    /// `GsonBuilder().setPrettyPrinting().create().toJson(element)`.
    pub fn to_pretty_string(&self) -> String {
        let mut out = String::new();
        write_element(&mut out, self, &Format { indent: true, html_safe: true, serialize_nulls: false }, 0);
        out
    }
}

struct Format {
    indent: bool,
    html_safe: bool,
    serialize_nulls: bool,
}

fn write_element(out: &mut String, element: &JsonElement, f: &Format, depth: usize) {
    let newline = |out: &mut String, depth: usize| {
        if f.indent {
            out.push('\n');
            for _ in 0..depth {
                out.push_str("  ");
            }
        }
    };
    match element {
        JsonElement::Null => out.push_str("null"),
        JsonElement::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        JsonElement::Number(n) => out.push_str(n),
        JsonElement::String(s) => write_string(out, s, f.html_safe),
        JsonElement::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(out, depth + 1);
                write_element(out, item, f, depth + 1);
            }
            newline(out, depth);
            out.push(']');
        }
        JsonElement::Object(obj) => {
            let members: Vec<_> = obj.iter().filter(|(_, v)| f.serialize_nulls || *v != &JsonElement::Null).collect();
            if members.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, (key, value)) in members.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(out, depth + 1);
                write_string(out, key, f.html_safe);
                out.push(':');
                if f.indent {
                    out.push(' ');
                }
                write_element(out, value, f, depth + 1);
            }
            newline(out, depth);
            out.push('}');
        }
    }
}

/// `JsonWriter.string`.
pub fn write_string(out: &mut String, s: &str, html_safe: bool) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            '<' | '>' | '&' | '=' | '\'' if html_safe => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Parse a whole document. `lenient` is Gson's behavior for `Gson.fromJson(Reader, …)` (comments, quotes, unquoted
/// names/strings, `;` and `=` separators); strict is `GsonHelper.parse`. `Ok(None)` is an empty document.
pub fn parse(text: &str, lenient: bool) -> Result<Option<JsonElement>, JsonError> {
    let mut p = Parser { chars: text.chars().collect(), pos: 0, lenient };
    p.skip_ws()?;
    if p.pos >= p.chars.len() {
        return Ok(None);
    }
    let value = p.parse_value(0)?;
    p.skip_ws()?;
    if p.pos < p.chars.len() {
        return Err(JsonError::new("JSON document was not fully consumed."));
    }
    Ok(Some(value))
}

/// `GsonHelper.parse(String)`: strict, must be a JSON object.
pub fn parse_object_strict(text: &str) -> Result<JsonObject, JsonError> {
    match parse(text, false)? {
        None => Err(JsonError::new("JSON data was null or empty")),
        Some(JsonElement::Object(o)) => Ok(o),
        Some(other) => {
            let kind = match other {
                JsonElement::Array(_) => "JsonArray",
                JsonElement::Null => "JsonNull",
                _ => "JsonPrimitive",
            };
            Err(JsonError::new(format!("Expected a com.google.gson.JsonObject but was com.google.gson.{kind}")))
        }
    }
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    lenient: bool,
}

const MAX_DEPTH: usize = 255;

impl Parser {
    fn err(&self, msg: &str) -> JsonError {
        JsonError::new(format!("{msg} at character {}", self.pos))
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_ws(&mut self) -> Result<(), JsonError> {
        loop {
            match self.peek() {
                Some(' ' | '\t' | '\n' | '\r') => self.pos += 1,
                Some('#') if self.lenient => self.skip_line(),
                Some('/') if self.lenient => match self.chars.get(self.pos + 1) {
                    Some('/') => self.skip_line(),
                    Some('*') => {
                        self.pos += 2;
                        loop {
                            match self.peek() {
                                None => return Err(self.err("Unterminated comment")),
                                Some('*') if self.chars.get(self.pos + 1) == Some(&'/') => {
                                    self.pos += 2;
                                    break;
                                }
                                _ => self.pos += 1,
                            }
                        }
                    }
                    _ => return Err(self.err("Use JsonReader.setStrictness(Strictness.LENIENT) to accept malformed JSON")),
                },
                _ => return Ok(()),
            }
        }
    }

    fn skip_line(&mut self) {
        while let Some(c) = self.peek() {
            self.pos += 1;
            if c == '\n' || c == '\r' {
                break;
            }
        }
    }

    fn parse_value(&mut self, depth: usize) -> Result<JsonElement, JsonError> {
        if depth > MAX_DEPTH {
            return Err(self.err("Nesting limit 255 reached"));
        }
        self.skip_ws()?;
        match self.peek() {
            None => Err(self.err("End of input")),
            Some('{') => {
                self.pos += 1;
                let mut obj = JsonObject::new();
                self.skip_ws()?;
                if self.peek() == Some('}') {
                    self.pos += 1;
                    return Ok(JsonElement::Object(obj));
                }
                loop {
                    self.skip_ws()?;
                    let key = self.parse_name()?;
                    self.skip_ws()?;
                    match self.peek() {
                        Some(':') => self.pos += 1,
                        Some('=') if self.lenient => {
                            self.pos += 1;
                            if self.peek() == Some('>') {
                                self.pos += 1;
                            }
                        }
                        _ => return Err(self.err("Expected ':'")),
                    }
                    let value = self.parse_value(depth + 1)?;
                    obj.add(&key, value);
                    self.skip_ws()?;
                    match self.peek() {
                        Some(',') => self.pos += 1,
                        Some(';') if self.lenient => self.pos += 1,
                        Some('}') => {
                            self.pos += 1;
                            return Ok(JsonElement::Object(obj));
                        }
                        _ => return Err(self.err("Expected ',' or '}'")),
                    }
                }
            }
            Some('[') => {
                self.pos += 1;
                let mut items = Vec::new();
                self.skip_ws()?;
                if self.peek() == Some(']') {
                    self.pos += 1;
                    return Ok(JsonElement::Array(items));
                }
                loop {
                    items.push(self.parse_value(depth + 1)?);
                    self.skip_ws()?;
                    match self.peek() {
                        Some(',') => self.pos += 1,
                        Some(';') if self.lenient => self.pos += 1,
                        Some(']') => {
                            self.pos += 1;
                            return Ok(JsonElement::Array(items));
                        }
                        _ => return Err(self.err("Expected ',' or ']'")),
                    }
                }
            }
            Some('"') => self.parse_quoted('"').map(JsonElement::String),
            Some('\'') if self.lenient => self.parse_quoted('\'').map(JsonElement::String),
            Some(_) => {
                let start = self.pos;
                let literal = self.parse_unquoted();
                if literal.is_empty() {
                    self.pos = start;
                    return Err(self.err("Expected value"));
                }
                if literal == "true" {
                    Ok(JsonElement::Bool(true))
                } else if literal == "false" {
                    Ok(JsonElement::Bool(false))
                } else if literal == "null" {
                    Ok(JsonElement::Null)
                } else if is_json_number(&literal) {
                    Ok(JsonElement::Number(literal))
                } else if self.lenient {
                    Ok(JsonElement::String(literal))
                } else {
                    self.pos = start;
                    Err(self.err("Use JsonReader.setStrictness(Strictness.LENIENT) to accept malformed JSON"))
                }
            }
        }
    }

    fn parse_name(&mut self) -> Result<String, JsonError> {
        match self.peek() {
            Some('"') => self.parse_quoted('"'),
            Some('\'') if self.lenient => self.parse_quoted('\''),
            _ if self.lenient => {
                let name = self.parse_unquoted();
                if name.is_empty() { Err(self.err("Expected name")) } else { Ok(name) }
            }
            _ => Err(self.err("Expected name")),
        }
    }

    fn parse_unquoted(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if matches!(c, '/' | '\\' | ';' | '#' | '=' | '{' | '}' | '[' | ']' | ':' | ',' | ' ' | '\t' | '\n' | '\r' | '\u{c}') {
                break;
            }
            self.pos += 1;
        }
        self.chars[start..self.pos].iter().collect()
    }

    fn parse_quoted(&mut self, quote: char) -> Result<String, JsonError> {
        self.pos += 1;
        let mut out = String::new();
        let mut pending_high: Option<u16> = None;
        let push_unit = |out: &mut String, pending: &mut Option<u16>, unit: u16| {
            if let Some(high) = pending.take() {
                if (0xDC00..0xE000).contains(&unit) {
                    let cp = 0x10000 + (((high as u32) - 0xD800) << 10) + (unit as u32 - 0xDC00);
                    out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                    return;
                }
                out.push('\u{FFFD}');
            }
            if (0xD800..0xDC00).contains(&unit) {
                *pending = Some(unit);
            } else if (0xDC00..0xE000).contains(&unit) {
                out.push('\u{FFFD}');
            } else {
                out.push(char::from_u32(unit as u32).unwrap_or('\u{FFFD}'));
            }
        };
        loop {
            let Some(c) = self.peek() else { return Err(self.err("Unterminated string")) };
            self.pos += 1;
            if c == quote {
                if pending_high.is_some() {
                    out.push('\u{FFFD}');
                }
                return Ok(out);
            }
            if c == '\\' {
                let Some(e) = self.peek() else { return Err(self.err("Unterminated escape sequence")) };
                self.pos += 1;
                let unit = match e {
                    'u' => {
                        let mut v = 0u32;
                        for _ in 0..4 {
                            let d = self.peek().and_then(|c| c.to_digit(16)).ok_or_else(|| self.err("Malformed Unicode escape"))?;
                            self.pos += 1;
                            v = v * 16 + d;
                        }
                        v as u16
                    }
                    't' => '\t' as u16,
                    'b' => 8,
                    'n' => '\n' as u16,
                    'r' => '\r' as u16,
                    'f' => 12,
                    '\n' | '\r' if self.lenient => e as u16,
                    '\'' | '"' | '\\' | '/' => e as u16,
                    _ if self.lenient => e as u16,
                    _ => return Err(self.err("Invalid escape sequence")),
                };
                push_unit(&mut out, &mut pending_high, unit);
            } else {
                if (c as u32) < 0x20 && !self.lenient {
                    return Err(self.err("Unescaped control character"));
                }
                if pending_high.take().is_some() {
                    out.push('\u{FFFD}');
                }
                out.push(c);
            }
        }
    }
}

/// JSON number grammar (`-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?`).
fn is_json_number(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    if b.get(i) == Some(&b'-') {
        i += 1;
    }
    match b.get(i) {
        Some(b'0') => i += 1,
        Some(b'1'..=b'9') => {
            while b.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
        }
        _ => return false,
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    i == b.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretty_matches_gson_layout() {
        let mut o = JsonObject::new();
        o.add_property_str("name", Some("a<b"));
        o.add_property_int("level", 4);
        o.add_property_bool("flag", false);
        o.add_property_str("reason", None);
        let arr = JsonElement::Array(vec![JsonElement::Object(o), JsonElement::Object(JsonObject::new())]);
        assert_eq!(
            arr.to_pretty_string(),
            "[\n  {\n    \"name\": \"a\\u003cb\",\n    \"level\": 4,\n    \"flag\": false\n  },\n  {}\n]"
        );
        assert_eq!(JsonElement::Array(vec![]).to_pretty_string(), "[]");
    }

    #[test]
    fn compact_strings() {
        let v = parse("{\"a\": [1, 2.50, true, null, \"x\\u00e9\\n\"], \"b\":{}}", false).unwrap().unwrap();
        assert_eq!(v.to_compact_string(), "{\"a\":[1,2.50,true,null,\"xé\\n\"],\"b\":{}}");
        let mut s = String::new();
        write_string(&mut s, "<&='>\u{1}\u{2028}", true);
        assert_eq!(s, "\"\\u003c\\u0026\\u003d\\u0027\\u003e\\u0001\\u2028\"");
    }

    #[test]
    fn strict_vs_lenient() {
        assert!(parse("{a:1}", false).is_err());
        assert!(parse("{'a': 'b'} ", true).unwrap().is_some());
        assert!(parse("[1,2] x", true).is_err());
        assert!(parse("// c\n[1;2]", true).is_ok());
        assert_eq!(parse("  ", true).unwrap(), None);
        assert!(parse_object_strict("[]").is_err());
        assert!(parse_object_strict("{\"a\":1,}").is_err());
    }

    #[test]
    fn duplicate_keys_keep_first_position() {
        let v = parse_object_strict("{\"a\":1,\"b\":2,\"a\":3}").unwrap();
        assert_eq!(JsonElement::Object(v).to_compact_string(), "{\"a\":3,\"b\":2}");
    }
}
