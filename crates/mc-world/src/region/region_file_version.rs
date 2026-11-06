//! `net.minecraft.world.level.chunk.storage.RegionFileVersion`: chunk payload compression ids.

use super::lz4;
use flate2::Compression;
use flate2::read::{MultiGzDecoder, ZlibDecoder};
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
            2 => Ok(Box::new(ZlibDecoder::new(Cursor::new(data)))),
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
}
