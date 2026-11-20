//! Hand-built DEFLATE streams: blocks of a chosen shape (stored data, a run of zero bytes in a fixed-code block) and the
//! corrupt blocks an inflater rejects, so a test decides which call of an inflater a failure lands in and with which
//! message.

/// The way a corrupt block is wrong, with the message zlib reports for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corrupt {
    /// `invalid block type`
    BlockType,
    /// `invalid stored block lengths`
    StoredLengths,
    /// `invalid distance too far back`: a copy from before the start of the output.
    DistanceTooFar,
    /// `invalid literal/length code`
    LiteralLengthCode,
    /// `invalid distance code`
    DistanceCode,
    /// `invalid code lengths set`
    CodeLengthsSet,
    /// `too many length or distance symbols`
    TooManySymbols,
}

/// A DEFLATE stream under construction: blocks are appended in order, none of them final.
#[derive(Default)]
pub struct Deflate {
    bytes: Vec<u8>,
    /// Bits used of the last byte (0 when it is full).
    used: u32,
}

impl Deflate {
    pub fn new() -> Self {
        Self::default()
    }

    /// The low `count` bits of `value`, least significant first (header fields and extra bits).
    fn bits(&mut self, value: u32, count: u32) {
        for bit in 0..count {
            if self.used == 0 {
                self.bytes.push(0);
            }
            *self.bytes.last_mut().expect("a byte") |= (((value >> bit) & 1) as u8) << self.used;
            self.used = (self.used + 1) % 8;
        }
    }

    /// A Huffman code of `count` bits, most significant first.
    fn code(&mut self, code: u32, count: u32) {
        for bit in (0..count).rev() {
            self.bits((code >> bit) & 1, 1);
        }
    }

    fn align(&mut self) {
        self.used = 0;
    }

    /// `data` in stored blocks (an empty slice is an empty stored block, a sync flush point).
    pub fn stored(mut self, data: &[u8]) -> Self {
        let mut chunks = data.chunks(usize::from(u16::MAX));
        for chunk in std::iter::once(chunks.next().unwrap_or(&[])).chain(chunks) {
            self.bits(0, 3);
            self.align();
            let length = chunk.len() as u16;
            self.bytes.extend(length.to_le_bytes());
            self.bytes.extend((!length).to_le_bytes());
            self.bytes.extend(chunk);
        }
        self
    }

    /// `count` zero bytes (at least one) in a block of fixed codes: a few input bytes that inflate to a lot of output.
    pub fn zeros(mut self, count: usize) -> Self {
        assert!(count >= 1);
        self.bits(0, 1);
        self.bits(1, 2);
        self.code(0x30, 8);
        let (copies, literals) = ((count - 1) / 258, (count - 1) % 258);
        for _ in 0..copies {
            // length 258, distance 1
            self.code(0xc5, 8);
            self.code(0, 5);
        }
        for _ in 0..literals {
            self.code(0x30, 8);
        }
        self.code(0, 7);
        self
    }

    /// A block that an inflater rejects.
    pub fn corrupt(mut self, kind: Corrupt) -> Self {
        match kind {
            Corrupt::BlockType => {
                self.bits(0, 1);
                self.bits(3, 2);
            }
            Corrupt::StoredLengths => {
                self.bits(0, 3);
                self.align();
                self.bytes.extend([1, 0, 1, 0]);
            }
            Corrupt::DistanceTooFar => {
                self.bits(0, 1);
                self.bits(1, 2);
                // length 3, distance code 29 with all extra bits set: 32768
                self.code(1, 7);
                self.code(29, 5);
                self.bits(8191, 13);
            }
            Corrupt::LiteralLengthCode => {
                self.bits(0, 1);
                self.bits(1, 2);
                self.code(0xc6, 8);
            }
            Corrupt::DistanceCode => {
                self.bits(0, 1);
                self.bits(1, 2);
                self.code(1, 7);
                self.code(30, 5);
            }
            Corrupt::CodeLengthsSet => {
                self.bits(0, 1);
                self.bits(2, 2);
                // 257 length codes, 1 distance code, 4 code length codes of length 1: more codes than one bit holds
                self.bits(0, 5);
                self.bits(0, 5);
                self.bits(0, 4);
                for _ in 0..4 {
                    self.bits(1, 3);
                }
            }
            Corrupt::TooManySymbols => {
                self.bits(0, 1);
                self.bits(2, 2);
                self.bits(31, 5);
                self.bits(0, 5);
                self.bits(0, 4);
            }
        }
        self
    }

    /// The bytes of the DEFLATE stream (the last byte is padded with zero bits).
    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }

    /// The stream in a gzip member without a trailer: a header (no name, no extra fields), the blocks and eight zero
    /// bytes, so that an inflater that reads ahead of what a block needs is not cut short.
    pub fn gzip(self) -> Vec<u8> {
        let mut member = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 0xff];
        member.extend(self.finish());
        member.extend([0; 8]);
        member
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::DeflateDecoder;
    use std::io::Read;

    fn inflate(deflate: Deflate) -> std::io::Result<Vec<u8>> {
        // some inflaters read ahead of what a block needs
        let mut bytes = deflate.finish();
        bytes.extend([0; 8]);
        let mut out = Vec::new();
        DeflateDecoder::new(bytes.as_slice()).read_to_end(&mut out)?;
        Ok(out)
    }

    #[test]
    fn blocks_inflate_to_what_they_hold() {
        let stream = Deflate::new().stored(b"abc").zeros(1000).stored(b"de");
        // the stream has no final block: flate2 reports the cut
        let mut expected = b"abc".to_vec();
        expected.extend([0; 1000]);
        expected.extend(b"de");
        let mut out = Vec::new();
        let _ = DeflateDecoder::new(stream.finish().as_slice()).read_to_end(&mut out);
        assert_eq!(out, expected);
    }

    #[test]
    fn corrupt_blocks_are_rejected() {
        for kind in [
            Corrupt::BlockType,
            Corrupt::StoredLengths,
            Corrupt::DistanceTooFar,
            Corrupt::LiteralLengthCode,
            Corrupt::DistanceCode,
            Corrupt::CodeLengthsSet,
            Corrupt::TooManySymbols,
        ] {
            let result = inflate(Deflate::new().stored(&[7; 40]).corrupt(kind));
            let error = result.expect_err(&format!("{kind:?}"));
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput, "{kind:?}");
        }
    }
}
