//! Errors raised by NBT IO and parsing. Variants map onto the Java exceptions vanilla throws.

use std::fmt;

/// Failure while reading or writing NBT.
///
/// * [`Eof`](Self::Eof), [`InvalidTagId`](Self::InvalidTagId), [`Io`](Self::Io) and
///   [`MalformedUtf`](Self::MalformedUtf) are `java.io.IOException`s.
/// * [`Reported`](Self::Reported) is vanilla's `ReportedNbtException` (an IO error inside a
///   compound element / root tag, which vanilla rethrows unchecked).
/// * [`Format`](Self::Format) is `NbtFormatException`, [`Accounter`](Self::Accounter) is
///   `NbtAccounterException` (both unchecked in vanilla).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NbtError {
    /// `EOFException`.
    Eof,
    /// `IOException("Invalid tag id: N")`.
    InvalidTagId(i32),
    /// Any other `IOException` with its message.
    Io(String),
    /// `UTFDataFormatException` (malformed or too long modified UTF-8).
    MalformedUtf(String),
    /// `ReportedNbtException`: an IO error wrapped while loading a named tag.
    Reported {
        /// Name of the tag being loaded (absent for a root tag).
        tag_name: Option<String>,
        /// `TagType.getName()` / numeric id of the tag being loaded.
        tag_type: String,
        cause: Box<NbtError>,
    },
    /// `NbtFormatException`.
    Format(String),
    /// `NbtAccounterException`.
    Accounter(String),
    /// `IllegalArgumentException` (e.g. negative accounted size).
    IllegalArgument(String),
}

impl NbtError {
    /// True for the variants that are `IOException` subclasses in Java.
    pub(crate) fn is_io(&self) -> bool {
        matches!(
            self,
            NbtError::Eof | NbtError::InvalidTagId(_) | NbtError::Io(_) | NbtError::MalformedUtf(_)
        )
    }
}

impl fmt::Display for NbtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NbtError::Eof => f.write_str("java.io.EOFException"),
            NbtError::InvalidTagId(id) => write!(f, "Invalid tag id: {id}"),
            NbtError::Io(m) | NbtError::MalformedUtf(m) | NbtError::Format(m) | NbtError::Accounter(m) | NbtError::IllegalArgument(m) => {
                f.write_str(m)
            }
            NbtError::Reported { tag_name, tag_type, cause } => match tag_name {
                Some(n) => write!(f, "Loading NBT data (tag {n:?} of type {tag_type}): {cause}"),
                None => write!(f, "Loading NBT data (tag type {tag_type}): {cause}"),
            },
        }
    }
}

impl std::error::Error for NbtError {}

impl From<std::io::Error> for NbtError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            NbtError::Eof
        } else {
            NbtError::Io(e.to_string())
        }
    }
}

pub type Result<T> = std::result::Result<T, NbtError>;
