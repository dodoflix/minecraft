//! Helpers shared by the argument parsers.

use mc_brigadier::{CommandExceptionType, CommandSyntaxException, ImmutableStringReader, StringReader};
use mc_nbt::snbt::SnbtError;
use std::sync::LazyLock;

/// Byte offset in `s` of UTF-16 offset `units` (Java `String` index).
pub fn utf16_to_byte(s: &str, units: usize) -> usize {
    let mut count = 0;
    for (i, c) in s.char_indices() {
        if count >= units {
            return i;
        }
        count += c.len_utf16();
    }
    s.len()
}

/// UTF-16 offset of byte offset `byte` in `s`.
pub fn byte_to_utf16(s: &str, byte: usize) -> usize {
    s[..byte.min(s.len())].encode_utf16().count()
}

/// Cursor of `reader` as a byte offset into its string.
pub fn reader_byte_cursor(reader: &StringReader) -> usize {
    utf16_to_byte(reader.get_string(), reader.get_cursor())
}

static SNBT_EXCEPTION_TYPE: LazyLock<CommandExceptionType> = LazyLock::new(CommandExceptionType::new);

/// The `CommandSyntaxException` a failed SNBT grammar parse raises.
pub fn snbt_exception(error: &SnbtError, input: &str) -> CommandSyntaxException {
    CommandSyntaxException::with_context(
        *SNBT_EXCEPTION_TYPE,
        crate::commands::message::literal_message(error.message()),
        input.to_owned(),
        error.cursor(),
    )
}

/// `reader.setCursor` from a byte offset.
pub fn set_reader_byte_cursor(reader: &mut StringReader, byte: usize) {
    let units = byte_to_utf16(reader.get_string(), byte);
    reader.set_cursor(units);
}
