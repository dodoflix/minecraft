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
        self.add(
            key,
            value.map_or(JsonElement::Null, |v| JsonElement::String(v.to_owned())),
        );
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
                text.parse::<f64>()
                    .map(|v| v as i32)
                    .map_err(|_| JsonError::new(format!("For input string: \"{text}\"")))
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
        write_element(
            &mut out,
            self,
            &Format {
                indent: false,
                html_safe: false,
                serialize_nulls: true,
            },
            0,
        );
        out
    }

    /// `new GsonBuilder().create().toJson(element)`: compact with HTML-safe escaping, nulls skipped.
    pub fn to_gson_string(&self) -> String {
        let mut out = String::new();
        write_element(
            &mut out,
            self,
            &Format {
                indent: false,
                html_safe: true,
                serialize_nulls: false,
            },
            0,
        );
        out
    }

    /// `new GsonBuilder().disableHtmlEscaping().create().toJson(element)`: compact, `\u2028`/`\u2029` escaped, nulls
    /// skipped in objects (`ByteBufCodecs.lenientJson`).
    pub fn to_gson_string_unescaped(&self) -> String {
        let mut out = String::new();
        write_element(
            &mut out,
            self,
            &Format {
                indent: false,
                html_safe: false,
                serialize_nulls: false,
            },
            0,
        );
        out
    }

    /// `GsonBuilder().setPrettyPrinting().create().toJson(element)`.
    pub fn to_pretty_string(&self) -> String {
        let mut out = String::new();
        write_element(
            &mut out,
            self,
            &Format {
                indent: true,
                html_safe: true,
                serialize_nulls: false,
            },
            0,
        );
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
            let members: Vec<_> = obj
                .iter()
                .filter(|(_, v)| f.serialize_nulls || *v != &JsonElement::Null)
                .collect();
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

/// Gson's `Strictness`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strictness {
    Lenient,
    LegacyStrict,
    Strict,
}

/// `Gson.fromJson(Reader, JsonElement-type)` with Gson's default strictness: the read runs LENIENT, the trailing
/// "fully consumed" check runs LEGACY_STRICT. `Ok(None)` is an empty document.
/// With `lenient == false` it is `GsonHelper.fromNullableJson` instead: STRICT, one value read, trailing data ignored,
/// an empty document is an `EOFException`.
pub fn parse(text: &str, lenient: bool) -> Result<Option<JsonElement>, JsonError> {
    if !lenient {
        let mut r = Reader::new(text, Strictness::Strict);
        return r.read_element().map(Some).map_err(ReadErr::into_error);
    }
    let mut r = Reader::new(text, Strictness::LegacyStrict);
    r.strictness = Strictness::Lenient;
    let element = match r.peek() {
        Err(e) if e.eof => return Ok(None),
        Err(e) => return Err(e.into_error()),
        Ok(_) => r.read_element().map_err(ReadErr::into_error)?,
    };
    r.strictness = Strictness::LegacyStrict;
    match r.peek() {
        Ok(Token::EndDocument) => Ok(Some(element)),
        Ok(_) => Err(JsonError::new("JSON document was not fully consumed.")),
        Err(e) => Err(e.into_error()),
    }
}

/// `JsonParser.parseString` (`LenientJsonParser.parse`): LENIENT read of one value (an empty document is `JsonNull`),
/// trailing data is rejected in LEGACY_STRICT mode unless the value is `null`. `Err` carries
/// `JsonSyntaxException.getMessage()`.
pub fn parse_lenient_json_parser(text: &str) -> Result<JsonElement, JsonError> {
    let mut r = Reader::new(text, Strictness::LegacyStrict);
    r.strictness = Strictness::Lenient;
    let element = match r.peek() {
        Err(e) if e.eof => JsonElement::Null,
        Err(e) => return Err(e.into_error()),
        Ok(_) => r.read_element().map_err(ReadErr::into_error)?,
    };
    r.strictness = Strictness::LegacyStrict;
    if element != JsonElement::Null {
        match r.peek() {
            Ok(Token::EndDocument) => {}
            Ok(_) => return Err(JsonError::new("Did not consume the entire document.")),
            Err(e) => return Err(e.into_error()),
        }
    }
    Ok(element)
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
            Err(JsonError::new(format!(
                "Expected a com.google.gson.JsonObject but was com.google.gson.{kind}; at path $"
            )))
        }
    }
}

/// `LazilyParsedNumber.intValue()` of a JSON number text: `Integer.parseInt`, then `(int) Long.parseLong`, then
/// `BigDecimal.intValue()` (truncate toward zero, low 32 bits). `Err` is the `NumberFormatException` message
/// (`NumberLimits`: more than 10000 characters or a scale of 10000 or more).
pub fn lazily_parsed_int_value(text: &str) -> Result<i32, String> {
    if let Ok(v) = text.parse::<i32>() {
        return Ok(v);
    }
    if let Ok(v) = text.parse::<i64>() {
        return Ok(v as i32);
    }
    if text.encode_utf16().count() > 10000 {
        let head: String = text.chars().take(30).collect();
        return Err(format!("Number string too large: {head}..."));
    }
    let unsupported = || format!("Number has unsupported scale: {text}");
    let (negative, rest) = match text.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, text),
    };
    let (mantissa, exp) = match rest.find(['e', 'E']) {
        Some(i) => (
            &rest[..i],
            rest[i + 1..]
                .trim_start_matches('+')
                .parse::<i64>()
                .map_err(|_| unsupported())?,
        ),
        None => (rest, 0),
    };
    let (int_digits, frac_digits) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let scale = frac_digits.len() as i64 - exp;
    if scale.abs() >= 10000 {
        return Err(unsupported());
    }
    let mut digits: Vec<u8> = int_digits.bytes().chain(frac_digits.bytes()).collect();
    if scale > 0 {
        digits.truncate(digits.len().saturating_sub(scale as usize));
    }
    let mut v = digits.iter().fold(0u32, |a, d| {
        a.wrapping_mul(10).wrapping_add(u32::from(d - b'0'))
    });
    if scale < 0 {
        let (mut base, mut e, mut p) = (10u32, (-scale) as u32, 1u32);
        while e > 0 {
            if e & 1 == 1 {
                p = p.wrapping_mul(base);
            }
            base = base.wrapping_mul(base);
            e >>= 1;
        }
        v = v.wrapping_mul(p);
    }
    Ok(if negative {
        v.wrapping_neg() as i32
    } else {
        v as i32
    })
}

/// `JsonToken`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    BeginObject,
    EndObject,
    BeginArray,
    EndArray,
    Name,
    String,
    Number,
    Boolean,
    Null,
    EndDocument,
}

/// A `MalformedJsonException`/`EOFException`; `message` is the exception's `toString()` (what
/// `new JsonSyntaxException(cause).getMessage()` shows).
struct ReadErr {
    eof: bool,
    message: String,
}

impl ReadErr {
    fn into_error(self) -> JsonError {
        JsonError(self.message)
    }
}

type RResult<T> = Result<T, ReadErr>;

const PEEKED_NONE: u8 = 0;
const PEEKED_BEGIN_OBJECT: u8 = 1;
const PEEKED_END_OBJECT: u8 = 2;
const PEEKED_BEGIN_ARRAY: u8 = 3;
const PEEKED_END_ARRAY: u8 = 4;
const PEEKED_TRUE: u8 = 5;
const PEEKED_FALSE: u8 = 6;
const PEEKED_NULL: u8 = 7;
const PEEKED_SINGLE_QUOTED: u8 = 8;
const PEEKED_DOUBLE_QUOTED: u8 = 9;
const PEEKED_UNQUOTED: u8 = 10;
const PEEKED_SINGLE_QUOTED_NAME: u8 = 12;
const PEEKED_DOUBLE_QUOTED_NAME: u8 = 13;
const PEEKED_UNQUOTED_NAME: u8 = 14;
const PEEKED_LONG: u8 = 15;
const PEEKED_NUMBER: u8 = 16;
const PEEKED_EOF: u8 = 17;

const EMPTY_ARRAY: u8 = 1;
const NONEMPTY_ARRAY: u8 = 2;
const EMPTY_OBJECT: u8 = 3;
const DANGLING_NAME: u8 = 4;
const NONEMPTY_OBJECT: u8 = 5;
const EMPTY_DOCUMENT: u8 = 6;
const NONEMPTY_DOCUMENT: u8 = 7;

/// `JsonReader`'s `BUFFER_SIZE`: the only place the buffer size is observable is the longest number token.
const BUFFER_SIZE: usize = 1024;
const DEFAULT_NESTING_LIMIT: usize = 255;

/// Port of `com.google.gson.stream.JsonReader` (2.14) over the whole input instead of a 1024-char window; the one
/// windowing effect that is observable (a numeric run of 1024 or more chars is no number) is kept.
struct Reader {
    buf: Vec<u16>,
    pos: usize,
    strictness: Strictness,
    line_number: usize,
    line_start: usize,
    peeked: u8,
    peeked_long: i64,
    peeked_number_length: usize,
    stack: Vec<u8>,
    path_names: Vec<Option<String>>,
    path_indices: Vec<usize>,
}

fn c16(c: char) -> u16 {
    c as u16
}

impl Reader {
    fn new(text: &str, strictness: Strictness) -> Reader {
        let buf: Vec<u16> = text.encode_utf16().collect();
        // the first fill skips a byte order mark
        let bom = usize::from(buf.first() == Some(&0xFEFF));
        Reader {
            buf,
            pos: bom,
            strictness,
            line_number: 0,
            line_start: bom,
            peeked: PEEKED_NONE,
            peeked_long: 0,
            peeked_number_length: 0,
            stack: vec![EMPTY_DOCUMENT],
            path_names: vec![None],
            path_indices: vec![0],
        }
    }

    fn limit(&self) -> usize {
        self.buf.len()
    }

    fn top(&self) -> usize {
        self.stack.len() - 1
    }

    fn location_string(&self) -> String {
        let line = self.line_number + 1;
        let column = self.pos as i64 - self.line_start as i64 + 1;
        format!(" at line {line} column {column} path {}", self.get_path())
    }

    fn get_path(&self) -> String {
        let mut result = String::from("$");
        for i in 0..self.stack.len() {
            match self.stack[i] {
                EMPTY_ARRAY | NONEMPTY_ARRAY => {
                    let _ = write!(result, "[{}]", self.path_indices[i]);
                }
                EMPTY_OBJECT | DANGLING_NAME | NONEMPTY_OBJECT => {
                    result.push('.');
                    if let Some(name) = &self.path_names[i] {
                        result.push_str(name);
                    }
                }
                _ => {}
            }
        }
        result
    }

    fn syntax_error(&self, message: &str) -> ReadErr {
        ReadErr {
            eof: false,
            message: format!(
                "com.google.gson.stream.MalformedJsonException: {message}{}\nSee https://github.com/google/gson/blob/main/Troubleshooting.md#malformed-json",
                self.location_string()
            ),
        }
    }

    fn check_lenient(&self) -> RResult<()> {
        if self.strictness != Strictness::Lenient {
            return Err(self.syntax_error(
                "Use JsonReader.setStrictness(Strictness.LENIENT) to accept malformed JSON",
            ));
        }
        Ok(())
    }

    fn push(&mut self, new_top: u8) -> RResult<()> {
        if self.stack.len() > DEFAULT_NESTING_LIMIT {
            return Err(ReadErr {
                eof: false,
                message: format!(
                    "com.google.gson.stream.MalformedJsonException: Nesting limit {DEFAULT_NESTING_LIMIT} reached{}",
                    self.location_string()
                ),
            });
        }
        self.stack.push(new_top);
        self.path_names.push(None);
        self.path_indices.push(0);
        Ok(())
    }

    fn pop(&mut self) {
        self.stack.pop();
        self.path_names.pop();
        self.path_indices.pop();
    }

    fn begin_array(&mut self) -> RResult<()> {
        self.expect_peeked(PEEKED_BEGIN_ARRAY)?;
        self.push(EMPTY_ARRAY)?;
        let t = self.top();
        self.path_indices[t] = 0;
        self.peeked = PEEKED_NONE;
        Ok(())
    }

    fn end_array(&mut self) -> RResult<()> {
        self.expect_peeked(PEEKED_END_ARRAY)?;
        self.pop();
        self.bump_index();
        self.peeked = PEEKED_NONE;
        Ok(())
    }

    fn begin_object(&mut self) -> RResult<()> {
        self.expect_peeked(PEEKED_BEGIN_OBJECT)?;
        self.push(EMPTY_OBJECT)?;
        self.peeked = PEEKED_NONE;
        Ok(())
    }

    fn end_object(&mut self) -> RResult<()> {
        self.expect_peeked(PEEKED_END_OBJECT)?;
        self.pop();
        self.bump_index();
        self.peeked = PEEKED_NONE;
        Ok(())
    }

    /// The `p == X` guard of `beginArray`/`endArray`/...: the element reader only calls them after a matching peek.
    fn expect_peeked(&mut self, want: u8) -> RResult<()> {
        let p = if self.peeked == PEEKED_NONE {
            self.do_peek()?
        } else {
            self.peeked
        };
        if p == want {
            Ok(())
        } else {
            Err(ReadErr {
                eof: false,
                message: format!(
                    "java.lang.IllegalStateException: unexpected token {p}{}",
                    self.location_string()
                ),
            })
        }
    }

    fn has_next(&mut self) -> RResult<bool> {
        let p = if self.peeked == PEEKED_NONE {
            self.do_peek()?
        } else {
            self.peeked
        };
        Ok(p != PEEKED_END_OBJECT && p != PEEKED_END_ARRAY && p != PEEKED_EOF)
    }

    fn peek(&mut self) -> RResult<Token> {
        let p = if self.peeked == PEEKED_NONE {
            self.do_peek()?
        } else {
            self.peeked
        };
        Ok(match p {
            PEEKED_BEGIN_OBJECT => Token::BeginObject,
            PEEKED_END_OBJECT => Token::EndObject,
            PEEKED_BEGIN_ARRAY => Token::BeginArray,
            PEEKED_END_ARRAY => Token::EndArray,
            PEEKED_TRUE | PEEKED_FALSE => Token::Boolean,
            PEEKED_NULL => Token::Null,
            PEEKED_SINGLE_QUOTED | PEEKED_DOUBLE_QUOTED | PEEKED_UNQUOTED => Token::String,
            PEEKED_SINGLE_QUOTED_NAME | PEEKED_DOUBLE_QUOTED_NAME | PEEKED_UNQUOTED_NAME => {
                Token::Name
            }
            PEEKED_LONG | PEEKED_NUMBER => Token::Number,
            _ => Token::EndDocument,
        })
    }

    fn do_peek(&mut self) -> RResult<u8> {
        let peek_stack = self.stack[self.top()];
        let t = self.top();
        if peek_stack == EMPTY_ARRAY {
            self.stack[t] = NONEMPTY_ARRAY;
        } else if peek_stack == NONEMPTY_ARRAY {
            match self.next_non_whitespace(true)? {
                44 => {}
                59 => self.check_lenient()?,
                93 => {
                    self.peeked = PEEKED_END_ARRAY;
                    return Ok(self.peeked);
                }
                _ => return Err(self.syntax_error("Unterminated array")),
            }
        } else {
            if peek_stack == EMPTY_OBJECT || peek_stack == NONEMPTY_OBJECT {
                self.stack[t] = DANGLING_NAME;
                if peek_stack == NONEMPTY_OBJECT {
                    match self.next_non_whitespace(true)? {
                        44 => {}
                        59 => self.check_lenient()?,
                        125 => {
                            self.peeked = PEEKED_END_OBJECT;
                            return Ok(self.peeked);
                        }
                        _ => return Err(self.syntax_error("Unterminated object")),
                    }
                }
                let c = self.next_non_whitespace(true)?;
                match c {
                    34 => {
                        self.peeked = PEEKED_DOUBLE_QUOTED_NAME;
                        return Ok(self.peeked);
                    }
                    39 => {
                        self.check_lenient()?;
                        self.peeked = PEEKED_SINGLE_QUOTED_NAME;
                        return Ok(self.peeked);
                    }
                    125 => {
                        if peek_stack != NONEMPTY_OBJECT {
                            self.peeked = PEEKED_END_OBJECT;
                            return Ok(self.peeked);
                        }
                        return Err(self.syntax_error("Expected name"));
                    }
                    _ => {
                        self.check_lenient()?;
                        self.pos -= 1;
                        if self.is_literal(c as u16)? {
                            self.peeked = PEEKED_UNQUOTED_NAME;
                            return Ok(self.peeked);
                        }
                        return Err(self.syntax_error("Expected name"));
                    }
                }
            }
            if peek_stack == DANGLING_NAME {
                self.stack[t] = NONEMPTY_OBJECT;
                match self.next_non_whitespace(true)? {
                    58 => {}
                    61 => {
                        self.check_lenient()?;
                        if self.pos < self.limit() && self.buf[self.pos] == c16('>') {
                            self.pos += 1;
                        }
                    }
                    _ => return Err(self.syntax_error("Expected ':'")),
                }
            } else if peek_stack == EMPTY_DOCUMENT {
                if self.strictness == Strictness::Lenient {
                    self.consume_non_execute_prefix()?;
                }
                self.stack[t] = NONEMPTY_DOCUMENT;
            } else if peek_stack == NONEMPTY_DOCUMENT {
                if self.next_non_whitespace(false)? == -1 {
                    self.peeked = PEEKED_EOF;
                    return Ok(self.peeked);
                }
                self.check_lenient()?;
                self.pos -= 1;
            }
        }
        let c = self.next_non_whitespace(true)?;
        match c {
            34 => {
                self.peeked = PEEKED_DOUBLE_QUOTED;
                Ok(self.peeked)
            }
            39 => {
                self.check_lenient()?;
                self.peeked = PEEKED_SINGLE_QUOTED;
                Ok(self.peeked)
            }
            91 => {
                self.peeked = PEEKED_BEGIN_ARRAY;
                Ok(self.peeked)
            }
            123 => {
                self.peeked = PEEKED_BEGIN_OBJECT;
                Ok(self.peeked)
            }
            93 | 44 | 59 => {
                if c == 93 && peek_stack == EMPTY_ARRAY {
                    self.peeked = PEEKED_END_ARRAY;
                    return Ok(self.peeked);
                }
                if peek_stack != EMPTY_ARRAY && peek_stack != NONEMPTY_ARRAY {
                    return Err(self.syntax_error("Unexpected value"));
                }
                self.check_lenient()?;
                self.pos -= 1;
                self.peeked = PEEKED_NULL;
                Ok(self.peeked)
            }
            _ => {
                self.pos -= 1;
                let result = self.peek_keyword()?;
                if result != 0 {
                    return Ok(result);
                }
                let result = self.peek_number()?;
                if result != 0 {
                    return Ok(result);
                }
                if !self.is_literal(self.buf[self.pos])? {
                    return Err(self.syntax_error("Expected value"));
                }
                self.check_lenient()?;
                self.peeked = PEEKED_UNQUOTED;
                Ok(self.peeked)
            }
        }
    }

    fn peek_keyword(&mut self) -> RResult<u8> {
        let (keyword, upper, peeking): (&[u8], &[u8], u8) = match self.buf[self.pos] {
            0x74 | 0x54 => (b"true", b"TRUE", PEEKED_TRUE),
            0x66 | 0x46 => (b"false", b"FALSE", PEEKED_FALSE),
            0x6e | 0x4e => (b"null", b"NULL", PEEKED_NULL),
            _ => return Ok(0),
        };
        let allows_upper = self.strictness != Strictness::Strict;
        let length = keyword.len();
        for i in 0..length {
            if self.pos + i >= self.limit() {
                return Ok(0);
            }
            let c = self.buf[self.pos + i];
            if !(c == u16::from(keyword[i]) || allows_upper && c == u16::from(upper[i])) {
                return Ok(0);
            }
        }
        if self.pos + length < self.limit() && self.is_literal(self.buf[self.pos + length])? {
            return Ok(0);
        }
        self.pos += length;
        self.peeked = peeking;
        Ok(peeking)
    }

    fn peek_number(&mut self) -> RResult<u8> {
        let (p, l) = (self.pos, self.limit());
        let mut value: i64 = 0;
        let mut negative = false;
        let mut fits_in_long = true;
        let mut last = 0u8;
        let mut i = 0usize;
        loop {
            if i == BUFFER_SIZE {
                return Ok(0);
            }
            if p + i == l {
                break;
            }
            let c = self.buf[p + i];
            match c {
                0x2b => {
                    if last != 5 {
                        return Ok(0);
                    }
                    last = 6;
                }
                0x2d => {
                    if last == 0 {
                        negative = true;
                        last = 1;
                    } else {
                        if last != 5 {
                            return Ok(0);
                        }
                        last = 6;
                    }
                }
                0x2e => {
                    if last != 2 {
                        return Ok(0);
                    }
                    last = 3;
                }
                0x45 | 0x65 => {
                    if last != 2 && last != 4 {
                        return Ok(0);
                    }
                    last = 5;
                }
                _ => {
                    if !(0x30..=0x39).contains(&c) {
                        if self.is_literal(c)? {
                            return Ok(0);
                        }
                        break;
                    }
                    let digit = i64::from(c - 0x30);
                    if last == 1 || last == 0 {
                        value = -digit;
                        last = 2;
                    } else if last == 2 {
                        if value == 0 {
                            return Ok(0);
                        }
                        let new_value = value.wrapping_mul(10).wrapping_sub(digit);
                        fits_in_long &= value > -922337203685477580
                            || value == -922337203685477580 && new_value < value;
                        value = new_value;
                    } else if last == 3 {
                        last = 4;
                    } else if last == 5 || last == 6 {
                        last = 7;
                    }
                }
            }
            i += 1;
        }
        if last != 2 || !fits_in_long || value == i64::MIN && !negative || value == 0 && negative {
            if last != 2 && last != 4 && last != 7 {
                return Ok(0);
            }
            self.peeked_number_length = i;
            self.peeked = PEEKED_NUMBER;
            Ok(self.peeked)
        } else {
            self.peeked_long = if negative { value } else { -value };
            self.pos += i;
            self.peeked = PEEKED_LONG;
            Ok(self.peeked)
        }
    }

    /// `isLiteral`: `checkLenient` for the chars that are only legal in lenient mode.
    fn is_literal(&self, c: u16) -> RResult<bool> {
        match c {
            0x23 | 0x2f | 0x3b | 0x3d | 0x5c => {
                self.check_lenient()?;
                Ok(false)
            }
            0x09 | 0x0a | 0x0c | 0x0d | 0x20 | 0x2c | 0x3a | 0x5b | 0x5d | 0x7b | 0x7d => Ok(false),
            _ => Ok(true),
        }
    }

    fn next_name(&mut self) -> RResult<String> {
        let p = if self.peeked == PEEKED_NONE {
            self.do_peek()?
        } else {
            self.peeked
        };
        let result = match p {
            PEEKED_UNQUOTED_NAME => self.next_unquoted_value()?,
            PEEKED_SINGLE_QUOTED_NAME => self.next_quoted_value(c16('\''))?,
            PEEKED_DOUBLE_QUOTED_NAME => self.next_quoted_value(c16('"'))?,
            _ => {
                return Err(ReadErr {
                    eof: false,
                    message: format!(
                        "java.lang.IllegalStateException: Expected a name{}",
                        self.location_string()
                    ),
                });
            }
        };
        self.peeked = PEEKED_NONE;
        let t = self.top();
        self.path_names[t] = Some(result.clone());
        Ok(result)
    }

    fn bump_index(&mut self) {
        let t = self.top();
        self.path_indices[t] += 1;
    }

    fn next_string(&mut self) -> RResult<String> {
        let p = if self.peeked == PEEKED_NONE {
            self.do_peek()?
        } else {
            self.peeked
        };
        let result = match p {
            PEEKED_UNQUOTED => self.next_unquoted_value()?,
            PEEKED_SINGLE_QUOTED => self.next_quoted_value(c16('\''))?,
            PEEKED_DOUBLE_QUOTED => self.next_quoted_value(c16('"'))?,
            PEEKED_LONG => self.peeked_long.to_string(),
            PEEKED_NUMBER => {
                let s = String::from_utf16_lossy(
                    &self.buf[self.pos..self.pos + self.peeked_number_length],
                );
                self.pos += self.peeked_number_length;
                s
            }
            _ => {
                return Err(ReadErr {
                    eof: false,
                    message: format!(
                        "java.lang.IllegalStateException: Expected a string{}",
                        self.location_string()
                    ),
                });
            }
        };
        self.peeked = PEEKED_NONE;
        self.bump_index();
        Ok(result)
    }

    fn next_boolean(&mut self) -> RResult<bool> {
        let p = if self.peeked == PEEKED_NONE {
            self.do_peek()?
        } else {
            self.peeked
        };
        if p == PEEKED_TRUE || p == PEEKED_FALSE {
            self.peeked = PEEKED_NONE;
            self.bump_index();
            Ok(p == PEEKED_TRUE)
        } else {
            Err(ReadErr {
                eof: false,
                message: format!(
                    "java.lang.IllegalStateException: Expected a boolean{}",
                    self.location_string()
                ),
            })
        }
    }

    fn next_null(&mut self) -> RResult<()> {
        self.expect_peeked(PEEKED_NULL)?;
        self.peeked = PEEKED_NONE;
        self.bump_index();
        Ok(())
    }

    fn next_quoted_value(&mut self, quote: u16) -> RResult<String> {
        let mut out: Vec<u16> = Vec::new();
        let l = self.limit();
        let mut p = self.pos;
        let mut start = p;
        while p < l {
            let c = self.buf[p];
            p += 1;
            if self.strictness == Strictness::Strict && c < 32 {
                return Err(self.syntax_error(
                    "Unescaped control characters (\\u0000-\\u001F) are not allowed in strict mode",
                ));
            }
            if c == quote {
                self.pos = p;
                out.extend_from_slice(&self.buf[start..p - 1]);
                return Ok(String::from_utf16_lossy(&out));
            }
            if c == 92 {
                self.pos = p;
                out.extend_from_slice(&self.buf[start..p - 1]);
                let e = self.read_escape_character()?;
                out.push(e);
                p = self.pos;
                start = p;
            } else if c == 10 {
                self.line_number += 1;
                self.line_start = p;
            }
        }
        self.pos = p;
        Err(self.syntax_error("Unterminated string"))
    }

    fn next_unquoted_value(&mut self) -> RResult<String> {
        let mut i = 0;
        while self.pos + i < self.limit() {
            match self.buf[self.pos + i] {
                0x09 | 0x0a | 0x0c | 0x0d | 0x20 | 0x2c | 0x3a | 0x5b | 0x5d | 0x7b | 0x7d => break,
                0x23 | 0x2f | 0x3b | 0x3d | 0x5c => {
                    self.check_lenient()?;
                    break;
                }
                _ => i += 1,
            }
        }
        let s = String::from_utf16_lossy(&self.buf[self.pos..self.pos + i]);
        self.pos += i;
        Ok(s)
    }

    fn next_non_whitespace(&mut self, throw_on_eof: bool) -> RResult<i32> {
        let l = self.limit();
        let mut p = self.pos;
        loop {
            if p == l {
                self.pos = p;
                if throw_on_eof {
                    return Err(ReadErr {
                        eof: true,
                        message: format!(
                            "java.io.EOFException: End of input{}",
                            self.location_string()
                        ),
                    });
                }
                return Ok(-1);
            }
            let c = self.buf[p];
            p += 1;
            if c == 10 {
                self.line_number += 1;
                self.line_start = p;
            } else if c != 32 && c != 13 && c != 9 {
                if c == 47 {
                    self.pos = p;
                    if p == l {
                        return Ok(i32::from(c));
                    }
                    self.check_lenient()?;
                    match self.buf[self.pos] {
                        0x2a => {
                            self.pos += 1;
                            if !self.skip_to(&[0x2a, 0x2f]) {
                                return Err(self.syntax_error("Unterminated comment"));
                            }
                            p = self.pos + 2;
                        }
                        0x2f => {
                            self.pos += 1;
                            self.skip_to_end_of_line();
                            p = self.pos;
                        }
                        _ => return Ok(i32::from(c)),
                    }
                } else {
                    if c != 35 {
                        self.pos = p;
                        return Ok(i32::from(c));
                    }
                    self.pos = p;
                    self.check_lenient()?;
                    self.skip_to_end_of_line();
                    p = self.pos;
                }
            }
        }
    }

    fn skip_to_end_of_line(&mut self) {
        while self.pos < self.limit() {
            let c = self.buf[self.pos];
            self.pos += 1;
            if c == 10 {
                self.line_number += 1;
                self.line_start = self.pos;
                break;
            } else if c == 13 {
                break;
            }
        }
    }

    fn skip_to(&mut self, to_find: &[u16]) -> bool {
        while self.pos + to_find.len() <= self.limit() {
            if self.buf[self.pos] == 10 {
                self.line_number += 1;
                self.line_start = self.pos + 1;
            } else if self.buf[self.pos..self.pos + to_find.len()] == *to_find {
                return true;
            }
            self.pos += 1;
        }
        false
    }

    fn read_escape_character(&mut self) -> RResult<u16> {
        if self.pos == self.limit() {
            return Err(self.syntax_error("Unterminated escape sequence"));
        }
        let escaped = self.buf[self.pos];
        self.pos += 1;
        match escaped {
            0x0a => {
                if self.strictness == Strictness::Strict {
                    return Err(
                        self.syntax_error("Cannot escape a newline character in strict mode")
                    );
                }
                self.line_number += 1;
                self.line_start = self.pos;
                Ok(escaped)
            }
            0x27 => {
                if self.strictness == Strictness::Strict {
                    return Err(self.syntax_error("Invalid escaped character \"'\" in strict mode"));
                }
                Ok(escaped)
            }
            0x22 | 0x2f | 0x5c => Ok(escaped),
            0x62 => Ok(8),
            0x66 => Ok(12),
            0x6e => Ok(10),
            0x72 => Ok(13),
            0x74 => Ok(9),
            0x75 => {
                if self.pos + 4 > self.limit() {
                    return Err(self.syntax_error("Unterminated escape sequence"));
                }
                let mut result: u16 = 0;
                for i in self.pos..self.pos + 4 {
                    let c = self.buf[i];
                    result <<= 4;
                    result += match c {
                        0x30..=0x39 => c - 0x30,
                        0x61..=0x66 => c - 0x61 + 10,
                        0x41..=0x46 => c - 0x41 + 10,
                        _ => {
                            let raw = String::from_utf16_lossy(&self.buf[self.pos..self.pos + 4]);
                            return Err(
                                self.syntax_error(&format!("Malformed Unicode escape \\u{raw}"))
                            );
                        }
                    };
                }
                self.pos += 4;
                Ok(result)
            }
            _ => Err(self.syntax_error("Invalid escape sequence")),
        }
    }

    fn consume_non_execute_prefix(&mut self) -> RResult<()> {
        self.next_non_whitespace(true)?;
        self.pos -= 1;
        if self.pos + 5 <= self.limit() {
            let p = self.pos;
            let prefix: Vec<u16> = ")]}'\n".encode_utf16().collect();
            if self.buf[p..p + 5] == prefix[..] {
                self.pos += 5;
            }
        }
        Ok(())
    }

    fn try_begin_nesting(&mut self, peeked: Token) -> RResult<Option<JsonElement>> {
        match peeked {
            Token::BeginArray => {
                self.begin_array()?;
                Ok(Some(JsonElement::Array(Vec::new())))
            }
            Token::BeginObject => {
                self.begin_object()?;
                Ok(Some(JsonElement::Object(JsonObject::new())))
            }
            _ => Ok(None),
        }
    }

    fn read_terminal(&mut self, peeked: Token) -> RResult<JsonElement> {
        match peeked {
            Token::String => self.next_string().map(JsonElement::String),
            Token::Number => self.next_string().map(JsonElement::Number),
            Token::Boolean => self.next_boolean().map(JsonElement::Bool),
            Token::Null => {
                self.next_null()?;
                Ok(JsonElement::Null)
            }
            _ => Err(ReadErr {
                eof: false,
                message: format!("java.lang.IllegalStateException: Unexpected token: {peeked:?}"),
            }),
        }
    }

    /// `JsonElementTypeAdapter.read` (iterative).
    fn read_element(&mut self) -> RResult<JsonElement> {
        let peeked = self.peek()?;
        let Some(first) = self.try_begin_nesting(peeked)? else {
            return self.read_terminal(peeked);
        };
        // containers being filled, with the member name each one is attached under once complete
        let mut stack: Vec<(JsonElement, Option<String>)> = Vec::new();
        let mut current = (first, None::<String>);
        loop {
            while !self.has_next()? {
                if matches!(current.0, JsonElement::Array(_)) {
                    self.end_array()?;
                } else {
                    self.end_object()?;
                }
                let Some(mut parent) = stack.pop() else {
                    return Ok(current.0);
                };
                let (finished, name) = current;
                attach(&mut parent.0, name, finished);
                current = parent;
            }
            let name = if matches!(current.0, JsonElement::Object(_)) {
                Some(self.next_name()?)
            } else {
                None
            };
            let peeked = self.peek()?;
            match self.try_begin_nesting(peeked)? {
                Some(value) => {
                    stack.push(current);
                    current = (value, name);
                }
                None => {
                    let value = self.read_terminal(peeked)?;
                    attach(&mut current.0, name, value);
                }
            }
        }
    }
}

fn attach(container: &mut JsonElement, name: Option<String>, value: JsonElement) {
    match (container, name) {
        (JsonElement::Array(items), _) => items.push(value),
        (JsonElement::Object(o), Some(name)) => o.add(&name, value),
        (JsonElement::Object(_), None) => unreachable!("object members are always named"),
        _ => unreachable!("only containers are attached to"),
    }
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
        let arr = JsonElement::Array(vec![
            JsonElement::Object(o),
            JsonElement::Object(JsonObject::new()),
        ]);
        assert_eq!(
            arr.to_pretty_string(),
            "[\n  {\n    \"name\": \"a\\u003cb\",\n    \"level\": 4,\n    \"flag\": false\n  },\n  {}\n]"
        );
        assert_eq!(JsonElement::Array(vec![]).to_pretty_string(), "[]");
    }

    #[test]
    fn compact_strings() {
        let v = parse(
            "{\"a\": [1, 2.50, true, null, \"x\\u00e9\\n\"], \"b\":{}}",
            false,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            v.to_compact_string(),
            "{\"a\":[1,2.50,true,null,\"xé\\n\"],\"b\":{}}"
        );
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
        assert_eq!(
            JsonElement::Object(v).to_compact_string(),
            "{\"a\":3,\"b\":2}"
        );
    }
}
