//! SNBT: parsing with vanilla's packrat grammar (`TagParser` / `SnbtGrammar`) and printing.
//!
//! Errors carry vanilla's (English) message and cursor, e.g.
//! `Expected literal ( at position 5: {a:1_<--[HERE]`.

mod grammar;
mod javanum;
mod packrat;
mod print;
mod unicode_names;

pub use print::{escape_without_quotes, print_compact, print_pretty, print_pretty_with, quote_and_escape};

use crate::{CompoundTag, Tag};
use packrat::{Ps, Reason, Val};
use std::fmt;

/// A `CommandSyntaxException` raised by the SNBT parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnbtError {
    message: String,
    /// UTF-16 code-unit offset (Java `String` index).
    cursor: usize,
    /// Byte offset of `cursor` in the input.
    byte_cursor: usize,
    context: String,
}

impl SnbtError {
    fn new(message: String, input: &str, units: &[u16], cursor: usize) -> Self {
        // CommandSyntaxException.getContext
        let c = cursor.min(units.len());
        let mut context = String::new();
        if c > 10 {
            context.push_str("...");
        }
        context.push_str(&String::from_utf16_lossy(&units[c.saturating_sub(10)..c]));
        context.push_str("<--[HERE]");
        let byte_cursor = utf16_to_byte(input, cursor);
        SnbtError { message, cursor, byte_cursor, context }
    }

    /// The bare message, without position context.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Cursor in UTF-16 code units (as in vanilla).
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Cursor as a byte offset into the parsed `&str`.
    pub fn byte_cursor(&self) -> usize {
        self.byte_cursor
    }

    /// `CommandSyntaxException.getContext()`.
    pub fn context(&self) -> &str {
        &self.context
    }
}

impl fmt::Display for SnbtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at position {}: {}", self.message, self.cursor, self.context)
    }
}

impl std::error::Error for SnbtError {}

fn utf16_to_byte(input: &str, units: usize) -> usize {
    let mut n = 0;
    for (i, c) in input.char_indices() {
        if n >= units {
            return i;
        }
        n += c.len_utf16();
    }
    input.len()
}

fn byte_to_utf16(input: &str, byte: usize) -> usize {
    input[..byte.min(input.len())].encode_utf16().count()
}

/// English text for a `snbt.parser.*` translation key (`en_us.json`). A key missing from the
/// language file renders as the key itself, like vanilla's `Language.getOrDefault`.
fn translate(key: &str, arg: Option<&str>) -> String {
    let template = match key {
        "snbt.parser.empty_key" => "Key cannot be empty",
        "snbt.parser.expected_binary_numeral" => "Expected a binary number",
        "snbt.parser.expected_decimal_numeral" => "Expected a decimal number",
        "snbt.parser.expected_float_type" => "Expected a floating point number",
        "snbt.parser.expected_hex_escape" => "Expected a character literal of length %s",
        "snbt.parser.expected_hex_numeral" => "Expected a hexadecimal number",
        "snbt.parser.expected_integer_type" => "Expected an integer number",
        "snbt.parser.expected_non_negative_number" => "Expected a non-negative number",
        "snbt.parser.expected_number_or_boolean" => "Expected a number or a boolean",
        "snbt.parser.expected_string_uuid" => "Expected a string representing a valid UUID",
        "snbt.parser.expected_unquoted_string" => "Expected a valid unquoted string",
        "snbt.parser.infinity_not_allowed" => "Non-finite numbers are not allowed",
        "snbt.parser.invalid_array_element_type" => "Invalid array element type",
        "snbt.parser.invalid_character_name" => "Invalid Unicode character name",
        "snbt.parser.invalid_codepoint" => "Invalid Unicode character value: %s",
        "snbt.parser.invalid_string_contents" => "Invalid string contents",
        "snbt.parser.invalid_unquoted_start" => "Unquoted strings can't start with digits 0-9, + or -",
        "snbt.parser.leading_zero_not_allowed" => "Decimal numbers can't start with 0",
        "snbt.parser.no_such_operation" => "No such operation: %s",
        "snbt.parser.number_parse_failure" => "Failed to parse number: %s",
        "argument.nbt.expected.compound" => "Expected compound tag",
        "argument.nbt.trailing" => "Unexpected trailing data",
        "snbt.parser.underscore_not_allowed" => "Underscore characters are not allowed at the start or end of a number",
        other => other,
    };
    match arg {
        Some(a) => template.replacen("%s", a, 1),
        None => template.to_string(),
    }
}

fn reason_message(reason: &Reason) -> String {
    match reason {
        Reason::Msg(key, arg) => translate(key, arg.as_deref()),
        Reason::Literal(text) => format!("Expected literal {text}"),
    }
}

/// Deepest bracket nesting accepted. Vanilla recurses on the Java stack and dies with a
/// `StackOverflowError` somewhere between ~500 and ~800 levels (JIT dependent); here the limit is
/// explicit so hostile command input cannot overflow the Rust stack.
const MAX_NESTING: usize = 1024;

/// Upper bound of `[`/`{`/`(` nesting in the input (string contents are not skipped, so it can
/// only over-estimate).
fn nesting_depth(units: &[u16], start: usize) -> usize {
    let (mut depth, mut max) = (0usize, 0usize);
    for &u in &units[start..] {
        match u {
            0x5b | 0x7b | 0x28 => {
                depth += 1;
                max = max.max(depth);
            }
            0x5d | 0x7d | 0x29 => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    max
}

/// Runs the grammar from UTF-16 offset `start`; returns the value and the end offset.
fn run(input: &str, units: &[u16], start: usize) -> Result<(Tag, usize), SnbtError> {
    let nesting = nesting_depth(units, start);
    if nesting > MAX_NESTING {
        return Err(SnbtError::new("Failed to parse: nesting too deep".to_string(), input, units, start));
    }
    // ~4 KiB of stack per nesting level (debug builds); shallow input runs on the caller's stack.
    if nesting <= 32 {
        return run_inner(input, units, start);
    }
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .stack_size((1 << 20) + nesting * (16 << 10))
            .spawn_scoped(s, || run_inner(input, units, start))
            .expect("spawn SNBT parser thread")
            .join()
            .unwrap_or_else(|p| std::panic::resume_unwind(p))
    })
}

fn run_inner(input: &str, units: &[u16], start: usize) -> Result<(Tag, usize), SnbtError> {
    let mut ps = Ps::new(&grammar::GRAMMAR, units, start);
    match ps.parse_top() {
        Some(Val::Tag(t)) => Ok((t, ps.cursor)),
        Some(_) => unreachable!("top rule yields a tag"),
        None => Err(match ps.failure() {
            Some((cursor, reason)) => SnbtError::new(reason_message(&reason), input, units, cursor),
            // vanilla: IllegalStateException("Failed to parse: ") — no error was recorded
            None => SnbtError::new("Failed to parse: ".to_string(), input, units, start),
        }),
    }
}

fn parse_fully_units(input: &str, units: &[u16]) -> Result<(Tag, usize), SnbtError> {
    let (tag, mut cursor) = run(input, units, 0)?;
    // reader.skipWhitespace(); if (reader.canRead()) throw ERROR_TRAILING_DATA
    while cursor < units.len() && packrat::is_whitespace(units[cursor]) {
        cursor += 1;
    }
    if cursor < units.len() {
        return Err(SnbtError::new(translate("argument.nbt.trailing", None), input, units, cursor));
    }
    Ok((tag, cursor))
}

fn expect_compound(tag: Tag, input: &str, units: &[u16], cursor: usize) -> Result<CompoundTag, SnbtError> {
    match tag {
        Tag::Compound(c) => Ok(c),
        _ => Err(SnbtError::new(translate("argument.nbt.expected.compound", None), input, units, cursor)),
    }
}

/// `TagParser.parseFully` with `NbtOps`: any SNBT value, whole input.
pub fn parse(input: &str) -> Result<Tag, SnbtError> {
    let units: Vec<u16> = input.encode_utf16().collect();
    parse_fully_units(input, &units).map(|(t, _)| t)
}

/// `TagParser.parseCompoundFully`.
pub fn parse_compound(input: &str) -> Result<CompoundTag, SnbtError> {
    let units: Vec<u16> = input.encode_utf16().collect();
    let (tag, cursor) = parse_fully_units(input, &units)?;
    expect_compound(tag, input, &units, cursor)
}

/// `TagParser.parseAsArgument`: parses one value starting at byte offset `start`; returns it with
/// the byte offset just past it (no trailing-data check, whitespace after is not consumed).
pub fn parse_as_argument(input: &str, start: usize) -> Result<(Tag, usize), SnbtError> {
    let units: Vec<u16> = input.encode_utf16().collect();
    let (tag, end) = run(input, &units, byte_to_utf16(input, start))?;
    Ok((tag, utf16_to_byte(input, end)))
}

/// `TagParser.parseCompoundAsArgument`.
pub fn parse_compound_as_argument(input: &str, start: usize) -> Result<(CompoundTag, usize), SnbtError> {
    let units: Vec<u16> = input.encode_utf16().collect();
    let (tag, end) = run(input, &units, byte_to_utf16(input, start))?;
    let compound = expect_compound(tag, input, &units, end)?;
    Ok((compound, utf16_to_byte(input, end)))
}
