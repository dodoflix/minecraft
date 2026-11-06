//! `net.jpountz.lz4.LZ4BlockOutputStream` / `LZ4BlockInputStream` framing (lz4-java 1.10) plus a
//! compact LZ4 block compressor/decompressor, as used by region file compression type 4.
//!
//! Frame: repeated `"LZ4Block" | token | compressed_len:i32le | decompressed_len:i32le |
//! checksum:i32le | payload`, terminated by an empty block (`token` raw, all lengths and checksum 0).
//! `token = method (0x10 raw / 0x20 lz4) | (compression_level - 10)`; blocks are 64 KiB.
//! The checksum is xxhash32 (seed `0x9747b28c`) of the decompressed block, masked to its low
//! 28 bits (`StreamingXXHash32.asChecksum()` returns `value & 0xFFFFFFFL`).

use std::io::{self, Read};
use twox_hash::XxHash32;

const MAGIC: &[u8; 8] = b"LZ4Block";
const MAGIC_LENGTH: usize = MAGIC.len();
const HEADER_LENGTH: usize = MAGIC_LENGTH + 1 + 4 + 4 + 4;
const COMPRESSION_LEVEL_BASE: u32 = 10;
const COMPRESSION_METHOD_RAW: u8 = 0x10;
const COMPRESSION_METHOD_LZ4: u8 = 0x20;
const DEFAULT_SEED: u32 = 0x9747b28c;
/// `LZ4BlockOutputStream(OutputStream)`: `1 << 16`.
pub const DEFAULT_BLOCK_SIZE: usize = 1 << 16;

const MIN_MATCH: usize = 4;
const LAST_LITERALS: usize = 5;
const MF_LIMIT: usize = 12;
const COPY_LENGTH: usize = 8;
const MAX_DISTANCE: usize = 65535;
const HASH_LOG: u32 = 12;

fn corrupted() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Stream is corrupted")
}

fn checksum(data: &[u8]) -> i32 {
    (XxHash32::oneshot(DEFAULT_SEED, data) & 0xFFF_FFFF) as i32
}

/// `LZ4BlockOutputStream.compressionLevel(blockSize)`.
fn compression_level(block_size: usize) -> u8 {
    let level = usize::BITS - (block_size - 1).leading_zeros();
    level.saturating_sub(COMPRESSION_LEVEL_BASE) as u8
}

/// Compresses `data` into an `LZ4BlockOutputStream` frame (default 64 KiB blocks), including the
/// terminating empty block written by `finish()`.
pub fn compress_frame(data: &[u8]) -> Vec<u8> {
    let level = compression_level(DEFAULT_BLOCK_SIZE);
    let mut out = Vec::with_capacity(data.len() / 2 + 2 * HEADER_LENGTH);
    let mut block = Vec::new();
    for chunk in data.chunks(DEFAULT_BLOCK_SIZE) {
        block.clear();
        compress_block(chunk, &mut block);
        // lz4-java: stored raw when compression does not shrink the block.
        let (method, payload) = if block.len() >= chunk.len() {
            (COMPRESSION_METHOD_RAW, chunk)
        } else {
            (COMPRESSION_METHOD_LZ4, block.as_slice())
        };
        out.extend_from_slice(MAGIC);
        out.push(method | level);
        out.extend_from_slice(&(payload.len() as i32).to_le_bytes());
        out.extend_from_slice(&(chunk.len() as i32).to_le_bytes());
        out.extend_from_slice(&checksum(chunk).to_le_bytes());
        out.extend_from_slice(payload);
    }
    out.extend_from_slice(MAGIC);
    out.push(COMPRESSION_METHOD_RAW | level);
    out.extend_from_slice(&[0; 12]);
    out
}

fn read_u32_le(src: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([src[i], src[i + 1], src[i + 2], src[i + 3]])
}

fn write_length(out: &mut Vec<u8>, mut len: usize) {
    while len >= 255 {
        out.push(255);
        len -= 255;
    }
    out.push(len as u8);
}

fn write_sequence(out: &mut Vec<u8>, literals: &[u8], match_len: Option<(usize, usize)>) {
    let token_pos = out.len();
    out.push(0);
    let mut token = (literals.len().min(15) as u8) << 4;
    if literals.len() >= 15 {
        write_length(out, literals.len() - 15);
    }
    out.extend_from_slice(literals);
    if let Some((offset, len)) = match_len {
        out.extend_from_slice(&(offset as u16).to_le_bytes());
        let ml = len - MIN_MATCH;
        token |= ml.min(15) as u8;
        if ml >= 15 {
            write_length(out, ml - 15);
        }
    }
    out[token_pos] = token;
}

/// Greedy single-pass LZ4 block compressor (blocks are at most 64 KiB so positions fit the table).
fn compress_block(src: &[u8], out: &mut Vec<u8>) {
    let len = src.len();
    let mut anchor = 0;
    if len > MF_LIMIT {
        let mut table = vec![0u32; 1 << HASH_LOG]; // position + 1, 0 = empty
        let match_limit = len - LAST_LITERALS;
        let mf_limit = len - MF_LIMIT;
        let mut i = 0;
        while i <= mf_limit {
            let seq = read_u32_le(src, i);
            let h = (seq.wrapping_mul(2654435761) >> (32 - HASH_LOG)) as usize;
            let candidate = table[h] as usize;
            table[h] = (i + 1) as u32;
            if candidate != 0 {
                let r = candidate - 1;
                if i - r <= MAX_DISTANCE && read_u32_le(src, r) == seq {
                    let mut m = MIN_MATCH;
                    while i + m < match_limit && src[r + m] == src[i + m] {
                        m += 1;
                    }
                    write_sequence(out, &src[anchor..i], Some((i - r, m)));
                    i += m;
                    anchor = i;
                    continue;
                }
            }
            i += 1;
        }
    }
    write_sequence(out, &src[anchor..], None);
}

/// `LZ4FastDecompressor.decompress(src, 0, dest, 0, dest_len)` (java-safe semantics): the source must
/// be consumed exactly.
fn decompress_block(src: &[u8], dest_len: usize) -> io::Result<Vec<u8>> {
    let mut out: Vec<u8> = Vec::with_capacity(dest_len);
    let mut s = 0;
    let read_len = |s: &mut usize, mut len: usize| -> io::Result<usize> {
        loop {
            let b = *src.get(*s).ok_or_else(corrupted)?;
            *s += 1;
            len += b as usize;
            if b != 255 {
                return Ok(len);
            }
        }
    };
    loop {
        let token = *src.get(s).ok_or_else(corrupted)?;
        s += 1;
        let mut literal_len = (token >> 4) as usize;
        if literal_len == 15 {
            literal_len = read_len(&mut s, literal_len)?;
        }
        let literal_end = s
            .checked_add(literal_len)
            .filter(|&e| e <= src.len())
            .ok_or_else(corrupted)?;
        let d_end = out
            .len()
            .checked_add(literal_len)
            .filter(|&e| e <= dest_len)
            .ok_or_else(corrupted)?;
        out.extend_from_slice(&src[s..literal_end]);
        s = literal_end;
        if d_end == dest_len {
            break;
        }
        if d_end + COPY_LENGTH > dest_len {
            return Err(corrupted());
        }
        if s + 2 > src.len() {
            return Err(corrupted());
        }
        let offset = u16::from_le_bytes([src[s], src[s + 1]]) as usize;
        s += 2;
        if offset == 0 || offset > out.len() {
            return Err(corrupted());
        }
        let mut match_len = (token & 15) as usize;
        if match_len == 15 {
            match_len = read_len(&mut s, match_len)?;
        }
        match_len += MIN_MATCH;
        if out.len() + match_len > dest_len {
            return Err(corrupted());
        }
        let start = out.len() - offset;
        for k in 0..match_len {
            out.push(out[start + k]);
        }
    }
    if s != src.len() {
        return Err(corrupted());
    }
    Ok(out)
}

/// `LZ4BlockInputStream` (stop on empty block): decodes block by block.
pub struct Lz4BlockReader {
    input: io::Cursor<Vec<u8>>,
    block: Vec<u8>,
    pos: usize,
    finished: bool,
}

impl Lz4BlockReader {
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            input: io::Cursor::new(data),
            block: Vec::new(),
            pos: 0,
            finished: false,
        }
    }

    fn refill(&mut self) -> io::Result<()> {
        let mut header = [0u8; HEADER_LENGTH];
        // readFully: EOFException on a short header, including a missing terminating block.
        self.input
            .read_exact(&mut header)
            .map_err(|_| io::Error::from(io::ErrorKind::UnexpectedEof))?;
        if &header[..MAGIC_LENGTH] != MAGIC {
            return Err(corrupted());
        }
        let token = header[MAGIC_LENGTH];
        let method = token & 0xF0;
        let level = COMPRESSION_LEVEL_BASE + (token & 0x0F) as u32;
        if method != COMPRESSION_METHOD_RAW && method != COMPRESSION_METHOD_LZ4 {
            return Err(corrupted());
        }
        let int_at =
            |o: usize| i32::from_le_bytes([header[o], header[o + 1], header[o + 2], header[o + 3]]);
        let compressed_len = int_at(MAGIC_LENGTH + 1);
        let original_len = int_at(MAGIC_LENGTH + 5);
        let check = int_at(MAGIC_LENGTH + 9);
        if original_len as i64 > 1i64 << level
            || original_len < 0
            || compressed_len < 0
            || (original_len == 0 && compressed_len != 0)
            || (original_len != 0 && compressed_len == 0)
            || (method == COMPRESSION_METHOD_RAW && original_len != compressed_len)
        {
            return Err(corrupted());
        }
        if original_len == 0 {
            if check != 0 {
                return Err(corrupted());
            }
            self.finished = true;
            return Ok(());
        }
        let mut payload = vec![0u8; compressed_len as usize];
        self.input
            .read_exact(&mut payload)
            .map_err(|_| io::Error::from(io::ErrorKind::UnexpectedEof))?;
        self.block = if method == COMPRESSION_METHOD_RAW {
            payload
        } else {
            decompress_block(&payload, original_len as usize)?
        };
        if checksum(&self.block) != check {
            return Err(corrupted());
        }
        self.pos = 0;
        Ok(())
    }
}

impl Read for Lz4BlockReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.finished {
            return Ok(0);
        }
        if buf.is_empty() {
            return Ok(0);
        }
        if self.pos == self.block.len() {
            self.block.clear();
            self.refill()?;
            if self.finished {
                return Ok(0);
            }
        }
        let n = buf.len().min(self.block.len() - self.pos);
        buf[..n].copy_from_slice(&self.block[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}
