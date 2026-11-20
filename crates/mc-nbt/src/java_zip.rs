//! The JDK's `InflaterInputStream` and `GZIPInputStream` over a byte slice, failing with the exceptions the JDK throws
//! (`ZipException`, `EOFException`) as the [`JavaException`] payload of the returned [`io::Error`].

use flate2::Crc;
use mc_core::java_exception::{EOF_EXCEPTION, JavaException, ZIP_EXCEPTION};
use std::io::{self, Read};

/// Bytes `InflaterInputStream` pulls from its source per refill (`GZIPInputStream(in)` uses it too).
const INFLATER_BUFFER: usize = 512;

fn zip_error(message: &str) -> io::Error {
    JavaException::new(ZIP_EXCEPTION, message).into_io(io::ErrorKind::InvalidData)
}

fn eof(message: Option<&str>) -> io::Error {
    let exception = match message {
        Some(m) => JavaException::new(EOF_EXCEPTION, m),
        None => JavaException::without_message(EOF_EXCEPTION),
    };
    exception.into_io(io::ErrorKind::UnexpectedEof)
}

/// `new InflaterInputStream(in)` over `data` from offset `base` (`new Inflater(zlib_header)`): the compressed bytes are
/// fed to the inflater 512 at a time, and a `read` returns the output of the first `inflate` call that produces any
/// (capped by the size asked for). A failing call loses what it decoded and everything after it, as for a Java caller.
/// The inflater is zlib's (`zlib-rs`): it stops decoding when the buffer it was given is full and rejects the same
/// streams, which flate2's default inflater does not.
pub struct InflaterInputStream<D> {
    data: D,
    /// Where the inflater's input starts.
    base: usize,
    /// End of the part of `data` the inflater was given so far.
    fed: usize,
    zlib_header: bool,
    inflater: zlib_rs::Inflate,
    finished: bool,
}

impl<D: AsRef<[u8]>> InflaterInputStream<D> {
    pub fn new(data: D, base: usize, zlib_header: bool) -> Self {
        Self {
            data,
            base,
            fed: base,
            zlib_header,
            inflater: zlib_rs::Inflate::new(zlib_header, 15),
            finished: false,
        }
    }

    /// `Inflater.reset()` for a stream that continues at `base`: the bytes already handed over stay handed over.
    fn restart(&mut self, base: usize) {
        self.base = base;
        self.fed = self.fed.max(base);
        self.inflater.reset(self.zlib_header);
        self.finished = false;
    }

    /// End of the input the inflater has consumed.
    fn consumed_end(&self) -> usize {
        self.base + self.inflater.total_in() as usize
    }
}

impl<D: AsRef<[u8]>> Read for InflaterInputStream<D> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        while !self.finished {
            let (in_before, out_before) = (self.consumed_end(), self.inflater.total_out());
            let input = &self.data.as_ref()[in_before..self.fed];
            let status = self
                .inflater
                .decompress(input, out, zlib_rs::InflateFlush::NoFlush)
                .map_err(|e| match e {
                    // `DataFormatException`: the JDK turns its message into a `ZipException`
                    zlib_rs::InflateError::DataError => zip_error(
                        self.inflater
                            .error_message()
                            .unwrap_or("Invalid ZLIB data format"),
                    ),
                    other => io::Error::new(io::ErrorKind::InvalidData, other.as_str()),
                })?;
            let produced = (self.inflater.total_out() - out_before) as usize;
            self.finished = status == zlib_rs::Status::StreamEnd;
            if produced > 0 {
                return Ok(produced);
            }
            if self.finished {
                continue;
            }
            let consumed = self.consumed_end();
            if consumed == self.fed {
                let data_len = self.data.as_ref().len();
                if self.fed == data_len {
                    return Err(eof(Some("Unexpected end of ZLIB input stream")));
                }
                self.fed = (self.fed + INFLATER_BUFFER).min(data_len);
            } else if consumed == in_before {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "inflater made no progress",
                ));
            }
        }
        Ok(0)
    }
}

/// `new GZIPInputStream(in)` over `data`: the header is read (and checked) on construction, the members are inflated
/// one after the other, and the trailer of a member is checked when the stream is read to its end.
pub struct GzipInputStream<D> {
    inflater: InflaterInputStream<D>,
    crc: Crc,
    eos: bool,
}

const FHCRC: u8 = 2;
const FEXTRA: u8 = 4;
const FNAME: u8 = 8;
const FCOMMENT: u8 = 16;

/// A cursor with the `readUByte`/`readUShort`/`skipBytes` of `GZIPInputStream`: a missing byte is a bare `EOFException`.
struct HeaderReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl HeaderReader<'_> {
    fn byte(&mut self) -> io::Result<u8> {
        let b = *self.data.get(self.pos).ok_or_else(|| eof(None))?;
        self.pos += 1;
        Ok(b)
    }

    fn short(&mut self) -> io::Result<u16> {
        let low = u16::from(self.byte()?);
        Ok(u16::from(self.byte()?) << 8 | low)
    }

    fn int(&mut self) -> io::Result<u32> {
        let low = u32::from(self.short()?);
        Ok(u32::from(self.short()?) << 16 | low)
    }

    fn skip(&mut self, n: usize) -> io::Result<()> {
        if self.data.len() - self.pos < n {
            self.pos = self.data.len();
            return Err(eof(None));
        }
        self.pos += n;
        Ok(())
    }
}

/// `GZIPInputStream.readHeader`: the offset just after the header at `start`. `Ok(None)` is the no-more-members
/// answer of `failOnEOF == false` when `start` is the end of the data.
fn read_header(data: &[u8], start: usize, fail_on_eof: bool) -> io::Result<Option<usize>> {
    if !fail_on_eof && start == data.len() {
        return Ok(None);
    }
    let mut r = HeaderReader { data, pos: start };
    if r.short()? != 0x8b1f {
        return Err(zip_error("Not in GZIP format"));
    }
    if r.byte()? != 8 {
        return Err(zip_error("Unsupported compression method"));
    }
    let flags = r.byte()?;
    r.skip(6)?;
    if flags & FEXTRA != 0 {
        let length = r.short()?;
        r.skip(usize::from(length))?;
    }
    for flag in [FNAME, FCOMMENT] {
        if flags & flag != 0 {
            while r.byte()? != 0 {}
        }
    }
    if flags & FHCRC != 0 {
        let mut crc = Crc::new();
        crc.update(&data[start..r.pos]);
        if r.short()? != (crc.sum() & 0xffff) as u16 {
            return Err(zip_error("Corrupt GZIP header"));
        }
    }
    Ok(Some(r.pos))
}

impl<D: AsRef<[u8]>> GzipInputStream<D> {
    pub fn new(data: D) -> io::Result<Self> {
        let body = read_header(data.as_ref(), 0, true)?.expect("a header is required");
        Ok(Self {
            inflater: InflaterInputStream::new(data, body, false),
            crc: Crc::new(),
            eos: false,
        })
    }

    /// `readTrailer`: checks the trailer of the member that ended; whether the stream ends with it.
    fn read_trailer(&mut self) -> io::Result<bool> {
        let end = self.inflater.consumed_end();
        let data = self.inflater.data.as_ref();
        let mut r = HeaderReader { data, pos: end };
        if r.int()? != self.crc.sum() || r.int()? != self.inflater.inflater.total_out() as u32 {
            return Err(zip_error("Corrupt GZIP trailer"));
        }
        // a malformed next header is ignored: the stream ends there
        match read_header(data, r.pos, false) {
            Ok(Some(next)) => {
                self.inflater.restart(next);
                self.crc = Crc::new();
                Ok(false)
            }
            _ => Ok(true),
        }
    }
}

impl<D: AsRef<[u8]>> Read for GzipInputStream<D> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.eos {
                return Ok(0);
            }
            let n = self.inflater.read(out)?;
            if n > 0 {
                self.crc.update(&out[..n]);
                return Ok(n);
            }
            if out.is_empty() {
                return Ok(0);
            }
            self.eos = self.read_trailer()?;
        }
    }
}

/// Everything `stream` yields before it fails, read in the 8192-byte refills of `FastBufferedInputStream`, and the
/// failure. A refill that fails delivers none of its bytes, which is what decides how far a Java caller gets. Reading
/// stops once more than `max` bytes are in (the caller reports the overflow).
pub fn drain(stream: &mut impl Read, max: Option<u64>) -> (Vec<u8>, Option<io::Error>) {
    let (mut data, mut buffer) = (Vec::new(), [0u8; 8192]);
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => return (data, None),
            Ok(n) => data.extend_from_slice(&buffer[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return (data, Some(e)),
        }
        if max.is_some_and(|max| data.len() as u64 > max) {
            return (data, None);
        }
    }
}

/// Whether `data` ends before the root tag's type byte and name are complete.
pub fn header_incomplete(data: &[u8]) -> bool {
    match data {
        [0, ..] => false,
        [_, hi, lo, ..] => data.len() < 3 + usize::from(u16::from_be_bytes([*hi, *lo])),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compress::gzip_compress;

    fn read_all(data: &[u8]) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        GzipInputStream::new(data)?.read_to_end(&mut out)?;
        Ok(out)
    }

    fn text(e: io::Error) -> String {
        e.to_string()
    }

    #[test]
    fn the_magic_is_checked_after_two_bytes() {
        let eof = "java.io.EOFException";
        assert_eq!(text(read_all(&[]).unwrap_err()), eof);
        assert_eq!(text(read_all(&[0x1f]).unwrap_err()), eof);
        for junk in [&[1u8, 2][..], &[1, 2, 3], &[0x1f, 0x8c, 8]] {
            assert_eq!(
                text(read_all(junk).unwrap_err()),
                "java.util.zip.ZipException: Not in GZIP format"
            );
        }
        assert_eq!(
            text(read_all(&[0x1f, 0x8b, 7]).unwrap_err()),
            "java.util.zip.ZipException: Unsupported compression method"
        );
        assert_eq!(text(read_all(&[0x1f, 0x8b, 8, 0, 0]).unwrap_err()), eof);
    }

    #[test]
    fn members_are_joined_and_a_truncated_stream_is_an_eof() {
        let mut two = gzip_compress(b"hello ");
        two.extend(gzip_compress(b"world"));
        assert_eq!(read_all(&two).unwrap(), b"hello world");
        let cut = &two[..14];
        assert_eq!(
            text(read_all(cut).unwrap_err()),
            "java.io.EOFException: Unexpected end of ZLIB input stream"
        );
        // a trailer that is damaged ends a complete read, a malformed next header does not
        let mut damaged = gzip_compress(b"abc");
        *damaged.last_mut().unwrap() ^= 1;
        assert_eq!(
            text(read_all(&damaged).unwrap_err()),
            "java.util.zip.ZipException: Corrupt GZIP trailer"
        );
        let mut junk = gzip_compress(b"abc");
        junk.extend([9, 9, 9]);
        assert_eq!(read_all(&junk).unwrap(), b"abc");
    }
}
