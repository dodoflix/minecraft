//! The JDK's `InflaterInputStream` and `GZIPInputStream` over a byte slice, failing with the exceptions the JDK throws
//! (`ZipException`, `EOFException`) as the [`JavaException`] payload of the returned [`io::Error`].

use crate::error::{NbtError, Result as NbtResult};
use crate::io::ByteInput;
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
                .or_else(|e| match e {
                    // `inflate` returning 0 with `needsDictionary()` sets `reachEOF`, so `read` returns -1
                    zlib_rs::InflateError::NeedDict { .. } => Ok(zlib_rs::Status::StreamEnd),
                    other => Err(other),
                })
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

/// `InflaterInputStream.skip`: reads (and drops) up to `n` bytes, 512 at a time; stops early at the end of the stream.
fn skip_by_reading(stream: &mut impl Read, n: u64) -> io::Result<u64> {
    let max = n.min(i32::MAX as u64);
    let mut buffer = [0u8; 512];
    let mut total = 0;
    while total < max {
        let want = (max - total).min(buffer.len() as u64) as usize;
        match stream.read(&mut buffer[..want])? {
            0 => break,
            read => total += read as u64,
        }
    }
    Ok(total)
}

/// `net.minecraft.util.FastBufferedInputStream` under a `DataInputStream`: an 8192-byte buffer refilled by one `read` of
/// the stream below, which a request of at least that size bypasses. The NBT readers take their bytes from it, so
/// where a failing stream stops a read is where the Java readers' requests put the refills.
pub struct FastBufferedInputStream<R> {
    inner: R,
    buffer: Box<[u8; 8192]>,
    position: usize,
    limit: usize,
    /// What `take` returns a slice of.
    scratch: Vec<u8>,
}

impl<R: Read> FastBufferedInputStream<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            buffer: Box::new([0; 8192]),
            position: 0,
            limit: 0,
            scratch: Vec::new(),
        }
    }

    fn fill(&mut self) -> io::Result<()> {
        self.limit = 0;
        self.position = 0;
        self.limit = self.inner.read(&mut self.buffer[..])?;
        Ok(())
    }

    /// `skip(count)`: what the buffer holds, or else what the stream below skips.
    fn skip_bytes(&mut self, count: u64) -> io::Result<u64> {
        let buffered = (self.limit - self.position) as u64;
        if buffered == 0 {
            return skip_by_reading(&mut self.inner, count);
        }
        let skipped = count.min(buffered);
        self.position += skipped as usize;
        Ok(skipped)
    }
}

impl<R: Read> Read for FastBufferedInputStream<R> {
    /// `read(output, 0, length)`; `Ok(0)` is the end of the stream.
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let mut buffered = self.limit - self.position;
        if buffered == 0 {
            if out.len() >= self.buffer.len() {
                return self.inner.read(out);
            }
            self.fill()?;
            buffered = self.limit;
            if buffered == 0 {
                return Ok(0);
            }
        }
        let length = out.len().min(buffered);
        out[..length].copy_from_slice(&self.buffer[self.position..self.position + length]);
        self.position += length;
        Ok(length)
    }
}

/// What one call of the stream below may be asked for in a single `take` (a call decodes at most what 512 input bytes
/// hold, far less than this, so a bigger request reads like the unbounded one of the JDK).
const TAKE_CHUNK: usize = 1 << 20;

impl<R: Read> ByteInput for FastBufferedInputStream<R> {
    /// `DataInputStream.readFully`.
    fn take(&mut self, n: usize) -> NbtResult<&[u8]> {
        let mut scratch = std::mem::take(&mut self.scratch);
        let mut got = 0;
        let result = loop {
            if got == n {
                break Ok(());
            }
            let want = (n - got).min(TAKE_CHUNK);
            if scratch.len() < got + want {
                scratch.resize(got + want, 0);
            }
            match self.read(&mut scratch[got..got + want]) {
                Ok(0) => break Err(NbtError::Eof),
                Ok(read) => got += read,
                Err(e) => break Err(NbtError::from(e)),
            }
        };
        self.scratch = scratch;
        result.map(|()| &self.scratch[..n])
    }

    /// `DataInputStream.skipBytes`.
    fn skip(&mut self, n: usize) -> NbtResult<()> {
        let mut total = 0;
        while total < n {
            match self.skip_bytes((n - total) as u64)? {
                0 => break,
                skipped => total += skipped as usize,
            }
        }
        Ok(())
    }

    fn reserve_hint(&self, count: usize) -> usize {
        count.min(1024)
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
    fn a_stream_that_needs_a_dictionary_ends() {
        let mut out = [0; 8];
        // a zlib header with FDICT set and the dictionary id
        let needs = [0x78, 0xbb, 1, 2, 3, 4, 0xff];
        let mut stream = InflaterInputStream::new(needs.as_slice(), 0, true);
        assert_eq!(stream.read(&mut out).unwrap(), 0);
        assert_eq!(stream.read(&mut out).unwrap(), 0);
        let cut = [0x78, 0xbb, 1, 2];
        let error = InflaterInputStream::new(cut.as_slice(), 0, true)
            .read(&mut out)
            .unwrap_err();
        assert_eq!(
            text(error),
            "java.io.EOFException: Unexpected end of ZLIB input stream"
        );
    }

    /// The root compound (`pad` zero bytes, an int, a string) and `after` zero bytes behind it, then an invalid block.
    fn refill_window(pad: usize, after: usize) -> Vec<u8> {
        let mut root = crate::CompoundTag::new();
        root.put("pad", crate::Tag::ByteArray(vec![0; pad]));
        root.put_int("x", 1);
        root.put_string("name", "level");
        let bytes = crate::nbt_io::to_bytes(&root);
        let mut entry = vec![7u8, 0, 3, b'p', b'a', b'd'];
        entry.extend((pad as i32).to_be_bytes());
        let at = bytes.windows(entry.len()).position(|w| w == entry).unwrap() + entry.len();
        mc_testkit::Deflate::new()
            .stored(&bytes[..at])
            .zeros(pad)
            .stored(&bytes[at + pad..])
            .zeros(after)
            .corrupt(mc_testkit::Corrupt::BlockType)
            .gzip()
    }

    fn read_root(data: &[u8]) -> Result<crate::CompoundTag, String> {
        let stream = FastBufferedInputStream::new(GzipInputStream::new(data).unwrap());
        let mut input = stream;
        crate::read_named(&mut input, &mut crate::NbtAccounter::unlimited_heap())
            .map_err(|e| e.java_exception().to_string())
    }

    /// A request of 8192 bytes or more bypasses the buffer, so the refills after it start where the request ended: the
    /// same stream is read whole or breaks, by where its end falls.
    #[test]
    fn a_failure_is_met_by_the_refill_that_reaches_it() {
        let failed = "net.minecraft.nbt.ReportedNbtException: Loading NBT data";
        // the root ends in a refill that reaches the invalid block, which fails and takes the bytes of the root with it
        assert_eq!(read_root(&refill_window(24540, 1)).unwrap_err(), failed);
        assert_eq!(read_root(&refill_window(21550, 3000)).unwrap_err(), failed);
        // the root ends in a refill that is full before the invalid block
        assert!(read_root(&refill_window(10000, 9000)).is_ok());
        assert!(read_root(&refill_window(5, 9000)).is_ok());
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
