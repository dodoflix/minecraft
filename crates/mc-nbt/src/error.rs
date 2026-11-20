//! Errors raised by NBT IO and parsing. Variants map onto the Java exceptions vanilla throws.

use mc_core::java_exception::JavaException;
use mc_core::java_string::JavaString;
use std::fmt;
use std::io;

/// Failure while reading or writing NBT.
///
/// * [`Eof`](Self::Eof), [`InvalidTagId`](Self::InvalidTagId), [`Io`](Self::Io), [`Stream`](Self::Stream) and
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
    MalformedUtf(JavaString),
    /// An `IOException` of the stream the bytes come from (`ZipException`, `EOFException` of a cut stream, an
    /// unreadable file), with the JDK's class and message.
    Stream(JavaException),
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
            NbtError::Eof
                | NbtError::InvalidTagId(_)
                | NbtError::Io(_)
                | NbtError::MalformedUtf(_)
                | NbtError::Stream(_)
        )
    }

    /// The Java exception this stands for.
    pub fn java_exception(&self) -> JavaException {
        match self {
            NbtError::Eof => JavaException::without_message("java.io.EOFException"),
            NbtError::InvalidTagId(id) => {
                JavaException::new("java.io.IOException", format!("Invalid tag id: {id}"))
            }
            NbtError::Io(m) => JavaException::new("java.io.IOException", m),
            NbtError::MalformedUtf(m) => {
                JavaException::new("java.io.UTFDataFormatException", m.as_lossy_str())
            }
            NbtError::Stream(exception) => exception.clone(),
            NbtError::Reported { .. } => JavaException::unchecked(
                "net.minecraft.nbt.ReportedNbtException",
                "Loading NBT data",
            ),
            NbtError::Format(m) => {
                JavaException::unchecked("net.minecraft.nbt.NbtFormatException", m)
            }
            NbtError::Accounter(m) => {
                JavaException::unchecked("net.minecraft.nbt.NbtAccounterException", m)
            }
            NbtError::IllegalArgument(m) => {
                JavaException::unchecked("java.lang.IllegalArgumentException", m)
            }
        }
    }
}

impl fmt::Display for NbtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NbtError::Eof => f.write_str("java.io.EOFException"),
            NbtError::InvalidTagId(id) => write!(f, "Invalid tag id: {id}"),
            NbtError::MalformedUtf(m) => f.write_str(m.as_lossy_str()),
            NbtError::Stream(exception) => write!(f, "{exception}"),
            NbtError::Io(m)
            | NbtError::Format(m)
            | NbtError::Accounter(m)
            | NbtError::IllegalArgument(m) => f.write_str(m),
            NbtError::Reported {
                tag_name,
                tag_type,
                cause,
            } => match tag_name {
                Some(n) => write!(
                    f,
                    "Loading NBT data (tag {n:?} of type {tag_type}): {cause}"
                ),
                None => write!(f, "Loading NBT data (tag type {tag_type}): {cause}"),
            },
        }
    }
}

impl std::error::Error for NbtError {}

/// An NBT failure as the [`io::Error`] its Java exception is: [`JavaException`] payload, so it prints as
/// `Throwable.toString()` and tells the unchecked exceptions apart.
impl From<NbtError> for io::Error {
    fn from(e: NbtError) -> io::Error {
        let kind = match e {
            NbtError::Eof => io::ErrorKind::UnexpectedEof,
            _ => io::ErrorKind::InvalidData,
        };
        e.java_exception().into_io(kind)
    }
}

impl From<io::Error> for NbtError {
    fn from(e: io::Error) -> Self {
        match e.get_ref() {
            Some(inner) => {
                if let Some(java) = inner.downcast_ref::<JavaException>() {
                    NbtError::Stream(java.clone())
                } else {
                    NbtError::Io(e.to_string())
                }
            }
            None if e.kind() == io::ErrorKind::UnexpectedEof => NbtError::Eof,
            None => NbtError::Io(e.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, NbtError>;
