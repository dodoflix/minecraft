//! `net.minecraft.world.level.chunk.storage.RegionFileVersion`: chunk payload compression ids.

use super::lz4;
use flate2::Compression;
use flate2::read::MultiGzDecoder;
use flate2::write::{GzEncoder, ZlibEncoder};
use std::io::{self, Cursor, Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Boxed decompressing reader (`RegionFileVersion.wrap(InputStream)`).
pub type ChunkInputStream = Box<dyn Read + Send>;

#[derive(Debug, PartialEq, Eq)]
pub struct RegionFileVersion {
    id: i32,
    option_name: Option<&'static str>,
}

pub static VERSION_GZIP: RegionFileVersion = RegionFileVersion {
    id: 1,
    option_name: None,
};
pub static VERSION_DEFLATE: RegionFileVersion = RegionFileVersion {
    id: 2,
    option_name: Some("deflate"),
};
pub static VERSION_NONE: RegionFileVersion = RegionFileVersion {
    id: 3,
    option_name: Some("none"),
};
pub static VERSION_LZ4: RegionFileVersion = RegionFileVersion {
    id: 4,
    option_name: Some("lz4"),
};
pub static VERSION_CUSTOM: RegionFileVersion = RegionFileVersion {
    id: 127,
    option_name: None,
};
pub static DEFAULT: &RegionFileVersion = &VERSION_DEFLATE;

static VERSIONS: [&RegionFileVersion; 5] = [
    &VERSION_GZIP,
    &VERSION_DEFLATE,
    &VERSION_NONE,
    &VERSION_LZ4,
    &VERSION_CUSTOM,
];
/// Index into [`VERSIONS`] (`volatile selected`).
static SELECTED: AtomicUsize = AtomicUsize::new(1);

impl RegionFileVersion {
    pub fn from_id(id: i32) -> Option<&'static RegionFileVersion> {
        VERSIONS.iter().copied().find(|v| v.id == id)
    }

    /// `configure(optionName)`: select by `region-file-compression` name; logs on an invalid name.
    pub fn configure(option_name: &str) {
        match VERSIONS
            .iter()
            .position(|v| v.option_name == Some(option_name))
        {
            Some(index) => SELECTED.store(index, Ordering::Relaxed),
            None => {
                let names: Vec<&str> = VERSIONS.iter().filter_map(|v| v.option_name).collect();
                log::error!(
                    "Invalid `region-file-compression` value `{}` in server.properties. Please use one of: {}",
                    option_name,
                    names.join(", ")
                );
            }
        }
    }

    pub fn get_selected() -> &'static RegionFileVersion {
        VERSIONS[SELECTED.load(Ordering::Relaxed)]
    }

    pub fn is_valid_version(version: i32) -> bool {
        Self::from_id(version).is_some()
    }

    pub fn get_id(&self) -> i32 {
        self.id
    }

    pub fn option_name(&self) -> Option<&'static str> {
        self.option_name
    }

    /// `wrap(InputStream)`: decompressing reader over the stored payload. The custom version has no
    /// wrapper (vanilla throws `UnsupportedOperationException`).
    pub fn wrap_input(&self, data: Vec<u8>) -> io::Result<ChunkInputStream> {
        match self.id {
            1 => Ok(Box::new(MultiGzDecoder::new(Cursor::new(data)))),
            2 => Ok(Box::new(JavaInflaterStream::new(data))),
            3 => Ok(Box::new(Cursor::new(data))),
            4 => Ok(Box::new(lz4::Lz4BlockReader::new(data))),
            _ => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "custom region file compression",
            )),
        }
    }

    /// `wrap(OutputStream)` applied to a complete payload: the compressed bytes to store after the
    /// 5-byte chunk header.
    pub fn compress(&self, raw: &[u8]) -> io::Result<Vec<u8>> {
        match self.id {
            1 => {
                let mut enc = GzEncoder::new(Vec::new(), Compression::default());
                enc.write_all(raw)?;
                enc.finish()
            }
            2 => {
                let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
                enc.write_all(raw)?;
                enc.finish()
            }
            3 => Ok(raw.to_vec()),
            4 => Ok(lz4::compress_frame(raw)),
            _ => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "custom region file compression",
            )),
        }
    }
}

/// Bytes `InflaterInputStream` pulls from its source per refill.
const INFLATER_BUFFER: usize = 512;

/// `new InflaterInputStream(in)` over a stored payload: the compressed bytes are fed to the inflater 512 at a time, and a
/// `read` returns the output of the first `inflate` call that produces any (capped by the size asked for). A failing call
/// loses what it decoded and everything after it, as for a Java caller. The inflater is zlib's (`zlib-rs`): it stops
/// decoding when the buffer it was given is full and rejects the same streams, which flate2's default inflater does not.
struct JavaInflaterStream {
    data: Vec<u8>,
    /// End of the part of `data` the inflater was given so far.
    fed: usize,
    inflater: zlib_rs::Inflate,
    finished: bool,
}

impl JavaInflaterStream {
    fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            fed: 0,
            inflater: zlib_rs::Inflate::new(true, 15),
            finished: false,
        }
    }
}

impl Read for JavaInflaterStream {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        while !self.finished {
            let (in_before, out_before) = (self.inflater.total_in(), self.inflater.total_out());
            let input = &self.data[in_before as usize..self.fed];
            let status = self
                .inflater
                .decompress(input, out, zlib_rs::InflateFlush::NoFlush)
                .map_err(|e| {
                    let message = self.inflater.error_message().unwrap_or(e.as_str());
                    io::Error::new(io::ErrorKind::InvalidData, message)
                })?;
            let produced = (self.inflater.total_out() - out_before) as usize;
            self.finished = status == zlib_rs::Status::StreamEnd;
            if produced > 0 {
                return Ok(produced);
            }
            if self.finished {
                continue;
            }
            let consumed = self.inflater.total_in() as usize;
            if consumed == self.fed {
                if self.fed == self.data.len() {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Unexpected end of ZLIB input stream",
                    ));
                }
                self.fed = (self.fed + INFLATER_BUFFER).min(self.data.len());
            } else if consumed as u64 == in_before {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "inflater made no progress",
                ));
            }
        }
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry() {
        assert_eq!(RegionFileVersion::from_id(2), Some(&VERSION_DEFLATE));
        assert_eq!(RegionFileVersion::from_id(127), Some(&VERSION_CUSTOM));
        assert!(RegionFileVersion::from_id(0).is_none() && !RegionFileVersion::is_valid_version(5));
        assert_eq!(RegionFileVersion::get_selected(), DEFAULT);
    }

    #[test]
    fn round_trips() {
        let raw: Vec<u8> = (0..200_000u32)
            .map(|i| (i % 251) as u8 ^ (i / 1000) as u8)
            .collect();
        for v in [&VERSION_GZIP, &VERSION_DEFLATE, &VERSION_NONE, &VERSION_LZ4] {
            let c = v.compress(&raw).unwrap();
            let mut back = Vec::new();
            v.wrap_input(c).unwrap().read_to_end(&mut back).unwrap();
            assert_eq!(back, raw, "version {}", v.id);
        }
        assert!(VERSION_CUSTOM.compress(&raw).is_err());
    }

    /// zlib rejects a match that reaches back before the start of the output ("invalid distance too far back"); the JDK
    /// throws `ZipException` for this stream.
    #[test]
    fn a_distance_before_the_start_is_invalid() {
        let stream = [0x78, 0x9c, 0x03, 0x02, 0x00, 0x00, 0x03, 0x00, 0x01];
        let mut out = Vec::new();
        assert!(
            VERSION_DEFLATE
                .wrap_input(stream.to_vec())
                .unwrap()
                .read_to_end(&mut out)
                .is_err()
        );
    }
}
