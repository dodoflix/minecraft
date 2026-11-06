//! Gzip-compressed fixtures: generated files over 1 MB are committed as `<name>.gz` (`docs/testing.md`).

use std::io::Read;
use std::path::Path;

use flate2::read::GzDecoder;

/// Decompresses a gzip-compressed UTF-8 fixture; panics with the reason when the bytes are not one.
pub fn gunzip(bytes: &[u8]) -> String {
    let mut text = String::new();
    GzDecoder::new(bytes)
        .read_to_string(&mut text)
        .unwrap_or_else(|e| panic!("fixture is not a gzip-compressed text file: {e}"));
    text
}

/// Reads and decompresses the gzip-compressed UTF-8 fixture at `path`.
pub fn read_gz(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    gunzip(&bytes)
}

/// The decompressed text of a gzip-compressed fixture embedded at compile time; the path is relative to the calling file,
/// as for `include_str!`.
#[macro_export]
macro_rules! include_gz {
    ($path:expr) => {
        $crate::gunzip(include_bytes!($path))
    };
}
