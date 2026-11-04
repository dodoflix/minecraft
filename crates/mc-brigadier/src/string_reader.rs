use std::sync::Arc;

use crate::exceptions::{CommandSyntaxException, built_in_exceptions};
use crate::utf16::is_java_whitespace;

const SYNTAX_ESCAPE: u16 = b'\\' as u16;
const SYNTAX_DOUBLE_QUOTE: u16 = b'"' as u16;
const SYNTAX_SINGLE_QUOTE: u16 = b'\'' as u16;

fn unit_to_char(u: u16) -> char {
    char::from_u32(u32::from(u)).unwrap_or('\u{FFFD}')
}

/// `com.mojang.brigadier.ImmutableStringReader`. All indices are UTF-16 code units, like Java.
/// `peek` returns a `char`; a lone surrogate unit (half of a non-BMP character) maps to U+FFFD.
pub trait ImmutableStringReader {
    fn get_string(&self) -> &str;
    fn get_remaining_length(&self) -> usize;
    fn get_total_length(&self) -> usize;
    fn get_cursor(&self) -> usize;
    fn get_read(&self) -> String;
    fn get_remaining(&self) -> String;
    fn can_read_n(&self, length: usize) -> bool;
    fn can_read(&self) -> bool;
    fn peek(&self) -> char;
    fn peek_at(&self, offset: usize) -> char;
}

#[derive(Debug)]
struct Data {
    string: String,
    units: Box<[u16]>,
}

/// `com.mojang.brigadier.StringReader`. `Clone` is the vanilla copy constructor and is cheap
/// (the text is shared).
#[derive(Clone, Debug)]
pub struct StringReader {
    data: Arc<Data>,
    cursor: usize,
}

impl StringReader {
    pub fn new(string: impl Into<String>) -> Self {
        let string = string.into();
        let units = string.encode_utf16().collect();
        Self { data: Arc::new(Data { string, units }), cursor: 0 }
    }

    pub fn set_cursor(&mut self, cursor: usize) {
        self.cursor = cursor;
    }

    fn unit(&self, index: usize) -> u16 {
        self.data.units[index]
    }

    fn slice(&self, start: usize, end: usize) -> String {
        if self.data.string.is_ascii() {
            self.data.string[start..end].to_owned()
        } else {
            String::from_utf16_lossy(&self.data.units[start..end])
        }
    }

    /// Reads one UTF-16 unit and advances (`char read()`).
    pub fn read(&mut self) -> char {
        let c = self.peek();
        self.cursor += 1;
        c
    }

    pub fn skip(&mut self) {
        self.cursor += 1;
    }

    pub fn is_allowed_number(c: char) -> bool {
        c.is_ascii_digit() || c == '.' || c == '-'
    }

    pub fn is_quoted_string_start(c: char) -> bool {
        c == '"' || c == '\''
    }

    pub fn is_allowed_in_unquoted_string(c: char) -> bool {
        c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '+')
    }

    pub fn skip_whitespace(&mut self) {
        while self.can_read() && is_java_whitespace(self.unit(self.cursor)) {
            self.skip();
        }
    }

    fn read_number_text(&mut self) -> (usize, String) {
        let start = self.cursor;
        while self.can_read() && Self::is_allowed_number(self.peek()) {
            self.skip();
        }
        (start, self.slice(start, self.cursor))
    }

    pub fn read_int(&mut self) -> Result<i32, CommandSyntaxException> {
        let (start, number) = self.read_number_text();
        if number.is_empty() {
            return Err(built_in_exceptions().reader_expected_int().create_with_context(self));
        }
        number.parse().map_err(|_| {
            self.cursor = start;
            built_in_exceptions().reader_invalid_int().create_with_context(self, number)
        })
    }

    pub fn read_long(&mut self) -> Result<i64, CommandSyntaxException> {
        let (start, number) = self.read_number_text();
        if number.is_empty() {
            return Err(built_in_exceptions().reader_expected_long().create_with_context(self));
        }
        number.parse().map_err(|_| {
            self.cursor = start;
            built_in_exceptions().reader_invalid_long().create_with_context(self, number)
        })
    }

    pub fn read_double(&mut self) -> Result<f64, CommandSyntaxException> {
        let (start, number) = self.read_number_text();
        if number.is_empty() {
            return Err(built_in_exceptions().reader_expected_double().create_with_context(self));
        }
        number.parse().map_err(|_| {
            self.cursor = start;
            built_in_exceptions().reader_invalid_double().create_with_context(self, number)
        })
    }

    pub fn read_float(&mut self) -> Result<f32, CommandSyntaxException> {
        let (start, number) = self.read_number_text();
        if number.is_empty() {
            return Err(built_in_exceptions().reader_expected_float().create_with_context(self));
        }
        number.parse().map_err(|_| {
            self.cursor = start;
            built_in_exceptions().reader_invalid_float().create_with_context(self, number)
        })
    }

    pub fn read_unquoted_string(&mut self) -> String {
        let start = self.cursor;
        while self.can_read() && Self::is_allowed_in_unquoted_string(self.peek()) {
            self.skip();
        }
        self.slice(start, self.cursor)
    }

    pub fn read_quoted_string(&mut self) -> Result<String, CommandSyntaxException> {
        if !self.can_read() {
            return Ok(String::new());
        }
        let next = self.peek();
        if !Self::is_quoted_string_start(next) {
            return Err(built_in_exceptions().reader_expected_start_of_quote().create_with_context(self));
        }
        self.skip();
        self.read_string_until(next)
    }

    pub fn read_string_until(&mut self, terminator: char) -> Result<String, CommandSyntaxException> {
        let terminator = u16::try_from(terminator as u32).ok();
        let mut result: Vec<u16> = Vec::new();
        let mut escaped = false;
        while self.can_read() {
            let c = self.unit(self.cursor);
            self.cursor += 1;
            if escaped {
                if Some(c) != terminator && c != SYNTAX_ESCAPE {
                    self.cursor -= 1;
                    return Err(built_in_exceptions()
                        .reader_invalid_escape()
                        .create_with_context(self, unit_to_char(c).to_string()));
                }
                result.push(c);
                escaped = false;
            } else if c == SYNTAX_ESCAPE {
                escaped = true;
            } else {
                if Some(c) == terminator {
                    return Ok(String::from_utf16_lossy(&result));
                }
                result.push(c);
            }
        }
        Err(built_in_exceptions().reader_expected_end_of_quote().create_with_context(self))
    }

    pub fn read_string(&mut self) -> Result<String, CommandSyntaxException> {
        if !self.can_read() {
            return Ok(String::new());
        }
        let next = self.unit(self.cursor);
        if next == SYNTAX_DOUBLE_QUOTE || next == SYNTAX_SINGLE_QUOTE {
            self.skip();
            self.read_string_until(unit_to_char(next))
        } else {
            Ok(self.read_unquoted_string())
        }
    }

    pub fn read_boolean(&mut self) -> Result<bool, CommandSyntaxException> {
        let start = self.cursor;
        let value = self.read_string()?;
        match value.as_str() {
            "" => Err(built_in_exceptions().reader_expected_bool().create_with_context(self)),
            "true" => Ok(true),
            "false" => Ok(false),
            _ => {
                self.cursor = start;
                Err(built_in_exceptions().reader_invalid_bool().create_with_context(self, value))
            }
        }
    }

    pub fn expect(&mut self, c: char) -> Result<(), CommandSyntaxException> {
        if self.can_read() && self.peek() == c {
            self.skip();
            Ok(())
        } else {
            Err(built_in_exceptions().reader_expected_symbol().create_with_context(self, c.to_string()))
        }
    }
}

impl ImmutableStringReader for StringReader {
    fn get_string(&self) -> &str {
        &self.data.string
    }

    fn get_remaining_length(&self) -> usize {
        self.data.units.len() - self.cursor
    }

    fn get_total_length(&self) -> usize {
        self.data.units.len()
    }

    fn get_cursor(&self) -> usize {
        self.cursor
    }

    fn get_read(&self) -> String {
        self.slice(0, self.cursor)
    }

    fn get_remaining(&self) -> String {
        self.slice(self.cursor, self.data.units.len())
    }

    fn can_read_n(&self, length: usize) -> bool {
        self.cursor + length <= self.data.units.len()
    }

    fn can_read(&self) -> bool {
        self.can_read_n(1)
    }

    fn peek(&self) -> char {
        unit_to_char(self.unit(self.cursor))
    }

    fn peek_at(&self, offset: usize) -> char {
        unit_to_char(self.unit(self.cursor + offset))
    }
}
