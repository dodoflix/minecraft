//! `net.minecraft.advancements.predicates.MinMaxBounds` (the command-facing part: reading from a `StringReader` and
//! matching). Codecs are not ported (only needed by predicates/advancements).

use crate::commands::message::simple_translatable;
use mc_brigadier::{BuiltInExceptionProvider, CommandSyntaxException, DynamicCommandExceptionType, ImmutableStringReader, SimpleCommandExceptionType, StringReader};
use std::sync::LazyLock;

pub static ERROR_EMPTY: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("argument.range.empty"));
pub static ERROR_SWAPPED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("argument.range.swapped"));

/// `MinMaxBounds.Bounds<T>`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Bounds<T> {
    pub min: Option<T>,
    pub max: Option<T>,
}

impl<T: PartialOrd + Copy> Bounds<T> {
    pub fn any() -> Self {
        Self { min: None, max: None }
    }

    pub fn exactly(v: T) -> Self {
        Self { min: Some(v), max: Some(v) }
    }

    pub fn between(min: T, max: T) -> Self {
        Self { min: Some(min), max: Some(max) }
    }

    pub fn at_least(v: T) -> Self {
        Self { min: Some(v), max: None }
    }

    pub fn at_most(v: T) -> Self {
        Self { min: None, max: Some(v) }
    }

    pub fn is_any(&self) -> bool {
        self.min.is_none() && self.max.is_none()
    }

    pub fn are_swapped(&self) -> bool {
        matches!((self.min, self.max), (Some(a), Some(b)) if a > b)
    }

    pub fn map<U>(&self, f: impl Fn(T) -> U) -> Bounds<U> {
        Bounds { min: self.min.map(&f), max: self.max.map(&f) }
    }

    /// `Bounds.fromReader`.
    pub fn from_reader(
        reader: &mut StringReader,
        converter: impl Fn(&str) -> Option<T>,
        parse_exc: impl Fn(&dyn BuiltInExceptionProvider) -> &DynamicCommandExceptionType<String>,
    ) -> Result<Bounds<T>, CommandSyntaxException> {
        if !reader.can_read() {
            return Err(ERROR_EMPTY.create_with_context(reader));
        }
        let start = reader.get_cursor();
        let result = (|| {
            let min = read_number(reader, &converter, &parse_exc)?;
            let max = if reader.can_read_n(2) && reader.peek() == '.' && reader.peek_at(1) == '.' {
                reader.skip();
                reader.skip();
                read_number(reader, &converter, &parse_exc)?
            } else {
                min
            };
            if min.is_none() && max.is_none() {
                Err(ERROR_EMPTY.create_with_context(reader))
            } else {
                Ok(Bounds { min, max })
            }
        })();
        result.map_err(|e| {
            reader.set_cursor(start);
            CommandSyntaxException::with_context(e.get_type(), e.get_raw_message().clone(), e.get_input().unwrap_or("").to_owned(), start)
        })
    }
}

fn read_number<T>(
    reader: &mut StringReader,
    converter: &impl Fn(&str) -> Option<T>,
    parse_exc: &impl Fn(&dyn BuiltInExceptionProvider) -> &DynamicCommandExceptionType<String>,
) -> Result<Option<T>, CommandSyntaxException> {
    let start = reader.get_cursor();
    while reader.can_read() && is_allowed_input_char(reader) {
        reader.skip();
    }
    let number = mc_brigadier::utf16::substring(reader.get_string(), start, reader.get_cursor());
    if number.is_empty() {
        return Ok(None);
    }
    match converter(&number) {
        Some(v) => Ok(Some(v)),
        None => {
            let provider = CommandSyntaxException::built_in_exceptions();
            Err(parse_exc(&*provider).create_with_context(reader, number))
        },
    }
}

fn is_allowed_input_char(reader: &StringReader) -> bool {
    let c = reader.peek();
    if c.is_ascii_digit() || c == '-' {
        true
    } else if c != '.' {
        false
    } else {
        !reader.can_read_n(2) || reader.peek_at(1) != '.'
    }
}

/// `MinMaxBounds.Doubles`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Doubles {
    pub bounds: Bounds<f64>,
    pub bounds_sqr: Bounds<f64>,
}

impl Doubles {
    pub fn new(bounds: Bounds<f64>) -> Self {
        Self { bounds, bounds_sqr: bounds.map(|d| d * d) }
    }

    pub fn any() -> Self {
        Self::new(Bounds::any())
    }

    pub fn exactly(v: f64) -> Self {
        Self::new(Bounds::exactly(v))
    }

    pub fn between(min: f64, max: f64) -> Self {
        Self::new(Bounds::between(min, max))
    }

    pub fn at_least(v: f64) -> Self {
        Self::new(Bounds::at_least(v))
    }

    pub fn at_most(v: f64) -> Self {
        Self::new(Bounds::at_most(v))
    }

    pub fn is_any(&self) -> bool {
        self.bounds.is_any()
    }

    pub fn matches(&self, value: f64) -> bool {
        if self.bounds.min.is_some_and(|m| m > value) { false } else { self.bounds.max.is_none_or(|m| !(m < value)) }
    }

    pub fn matches_sqr(&self, value_sqr: f64) -> bool {
        if self.bounds_sqr.min.is_some_and(|m| m > value_sqr) { false } else { self.bounds_sqr.max.is_none_or(|m| !(m < value_sqr)) }
    }

    pub fn from_reader(reader: &mut StringReader) -> Result<Doubles, CommandSyntaxException> {
        let start = reader.get_cursor();
        let bounds = Bounds::from_reader(reader, java_parse_double, |p| p.reader_invalid_double())?;
        if bounds.are_swapped() {
            reader.set_cursor(start);
            Err(ERROR_SWAPPED.create_with_context(reader))
        } else {
            Ok(Doubles::new(bounds))
        }
    }
}

/// `MinMaxBounds.FloatDegrees`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FloatDegrees {
    pub bounds: Bounds<f32>,
}

impl FloatDegrees {
    pub fn any() -> Self {
        Self { bounds: Bounds::any() }
    }

    pub fn is_any(&self) -> bool {
        self.bounds.is_any()
    }

    pub fn from_reader(reader: &mut StringReader) -> Result<FloatDegrees, CommandSyntaxException> {
        let bounds = Bounds::from_reader(reader, |s| java_parse_double(s).map(|d| d as f32), |p| p.reader_invalid_float())?;
        Ok(FloatDegrees { bounds })
    }
}

/// `MinMaxBounds.Ints`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Ints {
    pub bounds: Bounds<i32>,
    pub bounds_sqr: Bounds<i64>,
}

impl Ints {
    pub fn new(bounds: Bounds<i32>) -> Self {
        Self { bounds, bounds_sqr: bounds.map(|i| i64::from(i).wrapping_mul(i64::from(i))) }
    }

    pub fn any() -> Self {
        Self::new(Bounds::any())
    }

    pub fn exactly(v: i32) -> Self {
        Self::new(Bounds::exactly(v))
    }

    pub fn between(min: i32, max: i32) -> Self {
        Self::new(Bounds::between(min, max))
    }

    pub fn at_least(v: i32) -> Self {
        Self::new(Bounds::at_least(v))
    }

    pub fn at_most(v: i32) -> Self {
        Self::new(Bounds::at_most(v))
    }

    pub fn is_any(&self) -> bool {
        self.bounds.is_any()
    }

    pub fn matches(&self, value: i32) -> bool {
        if self.bounds.min.is_some_and(|m| m > value) { false } else { self.bounds.max.is_none_or(|m| m >= value) }
    }

    pub fn matches_sqr(&self, value_sqr: i64) -> bool {
        if self.bounds_sqr.min.is_some_and(|m| m > value_sqr) { false } else { self.bounds_sqr.max.is_none_or(|m| m >= value_sqr) }
    }

    pub fn from_reader(reader: &mut StringReader) -> Result<Ints, CommandSyntaxException> {
        let start = reader.get_cursor();
        let bounds = Bounds::from_reader(reader, |s| s.parse::<i32>().ok(), |p| p.reader_invalid_int())?;
        if bounds.are_swapped() {
            reader.set_cursor(start);
            Err(ERROR_SWAPPED.create_with_context(reader))
        } else {
            Ok(Ints::new(bounds))
        }
    }
}

/// `Double.parseDouble` restricted to what the input filter lets through (digits, `-`, `.`).
fn java_parse_double(s: &str) -> Option<f64> {
    // Java rejects strings Rust's parser also rejects here ("-", ".", "1.2.3"); both accept "1.", ".5", "-.5".
    s.parse::<f64>().ok()
}
