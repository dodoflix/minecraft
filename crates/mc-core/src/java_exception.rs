//! Java exceptions as the payload of an [`io::Error`]: the class name and message of the exception the JDK (or the
//! vanilla code) throws, printed like `Throwable.toString()`. Error types of other crates that stand for a Java
//! exception print the same way, so an error shown to a user carries Java's class and text wherever it surfaces.

use std::fmt;
use std::io;
use std::path::Path;

/// `java.io.EOFException`, the class named by streams that end early.
pub const EOF_EXCEPTION: &str = "java.io.EOFException";
/// `java.io.IOException`.
pub const IO_EXCEPTION: &str = "java.io.IOException";
/// `java.util.zip.ZipException`.
pub const ZIP_EXCEPTION: &str = "java.util.zip.ZipException";
/// `java.io.FileNotFoundException`.
pub const FILE_NOT_FOUND_EXCEPTION: &str = "java.io.FileNotFoundException";
/// `java.nio.file.FileAlreadyExistsException`.
pub const FILE_ALREADY_EXISTS_EXCEPTION: &str = "java.nio.file.FileAlreadyExistsException";
/// `java.nio.file.DirectoryNotEmptyException`.
pub const DIRECTORY_NOT_EMPTY_EXCEPTION: &str = "java.nio.file.DirectoryNotEmptyException";

/// A Java exception: its fully qualified class name and `getMessage()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaException {
    pub class: &'static str,
    pub message: Option<String>,
    /// A `RuntimeException` or `Error`: callers that handle `IOException` do not catch it.
    pub unchecked: bool,
}

impl JavaException {
    pub fn new(class: &'static str, message: impl Into<String>) -> JavaException {
        JavaException {
            class,
            message: Some(message.into()),
            unchecked: false,
        }
    }

    /// A `RuntimeException` with a message.
    pub fn unchecked(class: &'static str, message: impl Into<String>) -> JavaException {
        JavaException {
            unchecked: true,
            ..JavaException::new(class, message)
        }
    }

    /// An exception whose `getMessage()` is `null`.
    pub fn without_message(class: &'static str) -> JavaException {
        JavaException {
            class,
            message: None,
            unchecked: false,
        }
    }

    /// The exception as an [`io::Error`] of `kind`.
    pub fn into_io(self, kind: io::ErrorKind) -> io::Error {
        io::Error::new(kind, self)
    }

    /// The Java exception behind `e`: the payload itself, and for an error that carries none the `IOException` a
    /// stream read of an OS error is (`strerror`; the path of a failed open is not known here, see
    /// [`JavaException::from_file_error`]).
    pub fn of_io(e: &io::Error) -> JavaException {
        match e
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<JavaException>())
        {
            Some(exception) => exception.clone(),
            None if e.get_ref().is_some() => JavaException::new(IO_EXCEPTION, e.to_string()),
            None => JavaException::read_failure(e),
        }
    }

    /// Whether `e` stands for an unchecked Java exception, which a caller that handles `IOException` does not catch.
    pub fn is_unchecked(e: &io::Error) -> bool {
        e.get_ref()
            .and_then(|inner| inner.downcast_ref::<JavaException>())
            .is_some_and(|exception| exception.unchecked)
    }

    /// The `IOException(strerror)` a failed read of an open file throws (reading a directory: `Is a directory`).
    pub fn read_failure(e: &io::Error) -> JavaException {
        JavaException::new(IO_EXCEPTION, strerror(e))
    }

    /// The `FileNotFoundException("<path> (<strerror>)")` of `new FileOutputStream(file)`, whatever the reason it
    /// cannot open the file (a directory in its place, no permission, a missing parent).
    pub fn file_output_stream_failure(e: &io::Error, path: &Path) -> JavaException {
        JavaException::new(
            FILE_NOT_FOUND_EXCEPTION,
            format!("{} ({})", path.display(), strerror(e)),
        )
    }

    /// The `MalformedInputException` of a strict reader (`InputStreamReader`) that meets `bytes` that are not UTF-8:
    /// the decoder reports the malformed sequence it found.
    pub fn malformed_input(e: &std::string::FromUtf8Error) -> JavaException {
        let at = e.utf8_error().valid_up_to();
        let rest = &e.as_bytes()[at..];
        let length = match rest {
            // an encoded surrogate is one malformed sequence of three bytes
            [0xed, 0xa0..=0xbf, 0x80..=0xbf, ..] => 3,
            _ => e.utf8_error().error_len().unwrap_or(rest.len()),
        };
        JavaException::new(
            "java.nio.charset.MalformedInputException",
            format!("Input length = {length}"),
        )
    }

    /// The `MalformedInputException` of decoding `bytes` as UTF-8 without replacement through a `String`
    /// (`Files.readString`). `String.decodeUTF8_UTF16` reports its own lengths: a 3-byte lead with two bytes behind it
    /// is `3` whichever of them is bad, a 4-byte lead with three bytes behind it is `4`, and a sequence the end of the
    /// input cuts is `1` (`2` for a 3-byte lead whose second byte is bad).
    pub fn malformed_string_input(e: &std::string::FromUtf8Error) -> JavaException {
        let rest = &e.as_bytes()[e.utf8_error().valid_up_to()..];
        let length = match *rest {
            [0xe0..=0xef, _, _, ..] => 3,
            [lead @ 0xe0..=0xef, second]
                if (lead == 0xe0 && second & 0xe0 == 0x80) || second & 0xc0 != 0x80 =>
            {
                2
            }
            [0xf0..=0xf7, _, _, _, ..] => 4,
            _ => 1,
        };
        JavaException::new(
            "java.nio.charset.MalformedInputException",
            format!("Input length = {length}"),
        )
    }

    /// The exception `Files.newInputStream(path)` & co. throw for `e` (`UnixException.translateToIOException`).
    pub fn translate_file_error(e: &io::Error, path: &Path) -> JavaException {
        JavaException::translate_file_pair_error(e, path, None)
    }

    /// [`JavaException::translate_file_error`] of an operation on two files (`Files.move(file, other)`): the message
    /// names both (`file -> other`).
    pub fn translate_file_pair_error(
        e: &io::Error,
        path: &Path,
        other: Option<&Path>,
    ) -> JavaException {
        let mut shown = path.display().to_string();
        if let Some(other) = other {
            shown = format!("{shown} -> {}", other.display());
        }
        let reason = strerror(e);
        match (e.raw_os_error(), e.kind()) {
            (Some(40), _) => JavaException::new(
                "java.nio.file.FileSystemException",
                format!("{shown}: {reason} or unable to access attributes of symbolic link"),
            ),
            (Some(1), _) => file_system_exception(&shown, &reason),
            (_, io::ErrorKind::NotFound) => {
                JavaException::new("java.nio.file.NoSuchFileException", shown)
            }
            (_, io::ErrorKind::PermissionDenied) => {
                JavaException::new("java.nio.file.AccessDeniedException", shown)
            }
            (_, io::ErrorKind::AlreadyExists) => {
                JavaException::new(FILE_ALREADY_EXISTS_EXCEPTION, shown)
            }
            _ => file_system_exception(&shown, &reason),
        }
    }

    /// [`JavaException::translate_file_error`] as an [`io::Error`] of the same kind. An error that already is a Java
    /// exception is returned as it is.
    pub fn from_file_error(e: io::Error, path: &Path) -> io::Error {
        JavaException::from_file_pair_error(e, path, None)
    }

    /// [`JavaException::translate_file_pair_error`] as an [`io::Error`] of the same kind.
    pub fn from_file_pair_error(e: io::Error, path: &Path, other: Option<&Path>) -> io::Error {
        if e.get_ref().is_some() {
            return e;
        }
        let kind = e.kind();
        JavaException::translate_file_pair_error(&e, path, other).into_io(kind)
    }
}

fn file_system_exception(path: &str, reason: &str) -> JavaException {
    JavaException::new(
        "java.nio.file.FileSystemException",
        format!("{path}: {reason}"),
    )
}

/// `strerror` of an OS error: Rust appends ` (os error N)` to it.
fn strerror(e: &io::Error) -> String {
    let text = e.to_string();
    match (e.raw_os_error(), text.rfind(" (os error ")) {
        (Some(_), Some(at)) => text[..at].to_owned(),
        _ => text,
    }
}

impl fmt::Display for JavaException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.message {
            Some(message) => write!(f, "{}: {message}", self.class),
            None => f.write_str(self.class),
        }
    }
}

impl std::error::Error for JavaException {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_errors_print_as_the_jdk_throws_them() {
        let path = Path::new("/w/level.dat");
        let missing = io::Error::from_raw_os_error(2);
        assert_eq!(
            JavaException::from_file_error(missing, path).to_string(),
            "java.nio.file.NoSuchFileException: /w/level.dat"
        );
        let denied = io::Error::from_raw_os_error(13);
        assert_eq!(
            JavaException::from_file_error(denied, path).to_string(),
            "java.nio.file.AccessDeniedException: /w/level.dat"
        );
        let other = io::Error::from_raw_os_error(20);
        assert_eq!(
            JavaException::from_file_error(other, path).to_string(),
            "java.nio.file.FileSystemException: /w/level.dat: Not a directory"
        );
        let directory = io::Error::from_raw_os_error(21);
        assert_eq!(
            JavaException::file_output_stream_failure(&directory, path).to_string(),
            "java.io.FileNotFoundException: /w/level.dat (Is a directory)"
        );
        assert_eq!(
            JavaException::of_io(&directory).to_string(),
            "java.io.IOException: Is a directory"
        );
    }

    #[test]
    fn a_string_reports_its_own_malformed_length() {
        let length = |bytes: &[u8]| {
            let e = String::from_utf8(bytes.to_vec()).unwrap_err();
            JavaException::malformed_string_input(&e).to_string()
        };
        let at = |n: usize| format!("java.nio.charset.MalformedInputException: Input length = {n}");
        assert_eq!(length(b"{\xff}"), at(1));
        assert_eq!(length(b"{\xe1\x80A}"), at(3));
        assert_eq!(length(b"{\xe1A}"), at(3));
        assert_eq!(length(b"{\xed\xa0\x80}"), at(3));
        assert_eq!(length(b"{\xe1\x80"), at(1));
        assert_eq!(length(b"{\xe1A"), at(2));
        assert_eq!(length(b"{\xe0\x80"), at(2));
        assert_eq!(length(b"{\xf0\x80\x80\x80}"), at(4));
        assert_eq!(length(b"{\xf0\x9f\x98}"), at(4));
        assert_eq!(length(b"{\xf0\x9f\x98"), at(1));
        assert_eq!(length(b"{\xc3A}"), at(1));
        assert_eq!(length(b"{\xf8\x80\x80\x80\x80}"), at(1));
    }

    #[test]
    fn a_java_exception_stays_what_it_is() {
        let zip = JavaException::new(ZIP_EXCEPTION, "Not in GZIP format")
            .into_io(io::ErrorKind::InvalidData);
        let again = JavaException::from_file_error(zip, Path::new("x"));
        assert_eq!(
            JavaException::of_io(&again).to_string(),
            "java.util.zip.ZipException: Not in GZIP format"
        );
        assert_eq!(
            JavaException::without_message(EOF_EXCEPTION).to_string(),
            "java.io.EOFException"
        );
    }
}
