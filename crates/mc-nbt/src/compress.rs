//! gzip / zlib wrappers around the NBT readers and writers (`NbtIo.readCompressed` /
//! `writeCompressed`, and the zlib framing used by region-file chunk payloads).

use crate::accounter::NbtAccounter;
use crate::error::{NbtError, Result};
use crate::{CompoundTag, io};
use flate2::Compression;
use flate2::read::{MultiGzDecoder, ZlibDecoder};
use flate2::write::{GzEncoder, ZlibEncoder};
use std::io::{Read, Write};

fn inflate(mut decoder: impl Read, max_len: Option<u64>) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    match max_len {
        None => {
            decoder.read_to_end(&mut out)?;
        }
        Some(max) => {
            // Read one byte past the limit to detect overflow without unbounded allocation.
            (&mut decoder)
                .take(max.saturating_add(1))
                .read_to_end(&mut out)?;
            if out.len() as u64 > max {
                return Err(NbtError::Accounter(format!(
                    "Decompressed NBT is larger than the allowed {max} bytes"
                )));
            }
        }
    }
    Ok(out)
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

/// `NbtIo.readCompressed`: gzip-wrapped named compound root.
pub fn read_compressed(data: &[u8], acc: &mut NbtAccounter) -> Result<CompoundTag> {
    let raw = gzip_decompress(data, limit(acc))?;
    io::read(&mut raw.as_slice(), acc)
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
