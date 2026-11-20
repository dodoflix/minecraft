//! gzip / zlib wrappers around the NBT readers and writers (`NbtIo.readCompressed` /
//! `writeCompressed`, and the zlib framing used by region-file chunk payloads).

use crate::accounter::NbtAccounter;
use crate::error::{NbtError, Result};
use crate::{CompoundTag, io};
use flate2::Compression;
use flate2::read::{MultiGzDecoder, ZlibDecoder};
use flate2::write::{GzEncoder, ZlibEncoder};
use std::io::{Read, Write};

fn inflate(decoder: impl Read, max_len: Option<u64>) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    inflate_into(decoder, max_len, &mut out)?;
    Ok(out)
}

/// Like [`inflate`], keeping what was decoded before a failure in `out`.
fn inflate_into(mut decoder: impl Read, max_len: Option<u64>, out: &mut Vec<u8>) -> Result<()> {
    match max_len {
        None => {
            decoder.read_to_end(out)?;
        }
        Some(max) => {
            // Read one byte past the limit to detect overflow without unbounded allocation.
            (&mut decoder)
                .take(max.saturating_add(1))
                .read_to_end(out)?;
            if out.len() as u64 > max {
                return Err(NbtError::Accounter(format!(
                    "Decompressed NBT is larger than the allowed {max} bytes"
                )));
            }
        }
    }
    Ok(())
}

fn gzip_inflate(data: &[u8], max_len: Option<u64>, out: &mut Vec<u8>) -> Result<()> {
    inflate_into(MultiGzDecoder::new(data), max_len, out)
}

/// Decompresses gzip data (concatenated members allowed, like `GZIPInputStream`).
pub fn gzip_decompress(data: &[u8], max_len: Option<u64>) -> Result<Vec<u8>> {
    inflate(MultiGzDecoder::new(data), max_len)
}

pub fn gzip_compress(data: &[u8]) -> Vec<u8> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(data).expect("writing to a Vec cannot fail");
    enc.finish().expect("writing to a Vec cannot fail")
}

pub fn zlib_decompress(data: &[u8], max_len: Option<u64>) -> Result<Vec<u8>> {
    inflate(ZlibDecoder::new(data), max_len)
}

pub fn zlib_compress(data: &[u8]) -> Vec<u8> {
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(data).expect("writing to a Vec cannot fail");
    enc.finish().expect("writing to a Vec cannot fail")
}

/// A finite accounter quota bounds the decompressed size (accounted bytes always exceed wire bytes).
fn limit(acc: &NbtAccounter) -> Option<u64> {
    (acc.quota() != i64::MAX).then(|| acc.quota().max(0) as u64)
}

/// `NbtIo.readCompressed`: gzip-wrapped named compound root. The stream inflates while the tag is read, so a broken
/// stream is only an error where the reader runs out of decoded bytes, and then it is the error of that read (a
/// `ReportedNbtException` inside the root compound, a plain exception before it).
pub fn read_compressed(data: &[u8], acc: &mut NbtAccounter) -> Result<CompoundTag> {
    let mut raw = Vec::new();
    let failure = match gzip_inflate(data, limit(acc), &mut raw) {
        Ok(()) => None,
        Err(e) if !e.is_io() => return Err(e),
        Err(e) => Some(e),
    };
    let read = io::read(&mut raw.as_slice(), acc);
    match (read, failure) {
        (Err(error), Some(cause)) => Err(error.with_eof_replaced(cause)),
        (read, _) => read,
    }
}

/// `NbtIo.writeCompressed`: gzip-wrapped named compound root.
pub fn write_compressed(tag: &CompoundTag) -> Vec<u8> {
    gzip_compress(&io::to_bytes(tag))
}

/// Zlib-wrapped named compound root (region file compression type 2).
pub fn read_compressed_zlib(data: &[u8], acc: &mut NbtAccounter) -> Result<CompoundTag> {
    let raw = zlib_decompress(data, limit(acc))?;
    io::read(&mut raw.as_slice(), acc)
}

pub fn write_compressed_zlib(tag: &CompoundTag) -> Vec<u8> {
    zlib_compress(&io::to_bytes(tag))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gzip_of_compound() -> Vec<u8> {
        let mut tag = CompoundTag::new();
        for i in 0..200 {
            tag.put_int(format!("key{i}"), i);
        }
        write_compressed(&tag)
    }

    /// The stream inflates while the tag is read: a cut stream fails inside the root compound (reported), an empty one
    /// before it, and a damaged trailer after the tag was read is never looked at.
    #[test]
    fn broken_gzip_fails_where_the_reader_runs_out() {
        let full = gzip_of_compound();
        let read = |bytes: &[u8]| read_compressed(bytes, &mut NbtAccounter::unlimited_heap());
        assert!(read(&full).is_ok());
        assert!(matches!(
            read(&full[..full.len() / 2]),
            Err(NbtError::Reported { .. })
        ));
        assert_eq!(read(&[]), Err(NbtError::Eof));
        let mut damaged = full;
        *damaged.last_mut().expect("trailer") ^= 0xff;
        assert!(read(&damaged).is_ok());
    }
}
