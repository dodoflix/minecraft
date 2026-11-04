//! `net.minecraft.world.level.chunk.storage.RegionFile`: one `r.<x>.<z>.mca` Anvil region file.
//!
//! Layout: 4096-byte offset table (1024 big-endian ints: `sector_number << 8 | num_sectors`),
//! 4096-byte timestamp table, then 4096-byte sectors. A chunk is stored as `length:i32be |
//! compression id:u8 | payload`; payloads of 256+ sectors go to `c.<x>.<z>.mcc` next to the region
//! file, with a 1-sector stub whose id byte has bit 128 set.

use super::region_bitmap::RegionBitmap;
use super::region_file_version::{ChunkInputStream, RegionFileVersion, VERSION_CUSTOM};
use super::region_storage_info::RegionStorageInfo;
use mc_core::ident::Identifier;
use mc_core::pos::chunk_pos::ChunkPos;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const SECTOR_BYTES: usize = 4096;
/// `SECTOR_INTS`: entries per header table.
pub const SECTOR_INTS: usize = 1024;
const CHUNK_HEADER_SIZE: usize = 5;
const HEADER_SIZE: usize = 8192;
const EXTERNAL_STREAM_FLAG: u8 = 128;
const EXTERNAL_CHUNK_THRESHOLD: usize = 256;

pub struct RegionFile {
    #[allow(dead_code)]
    info: RegionStorageInfo,
    path: PathBuf,
    file: File,
    external_file_dir: PathBuf,
    version: &'static RegionFileVersion,
    offsets: Box<[i32; SECTOR_INTS]>,
    timestamps: Box<[i32; SECTOR_INTS]>,
    used_sectors: RegionBitmap,
}

fn get_timestamp() -> i32 {
    (SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis()) / 1000) as i32
}

fn is_external_stream_chunk(version: u8) -> bool {
    version & EXTERNAL_STREAM_FLAG != 0
}

fn get_external_chunk_version(version: u8) -> u8 {
    version & !EXTERNAL_STREAM_FLAG
}

fn pack_sector_offset(index: usize, size: usize) -> i32 {
    ((index << 8) | size) as i32
}

fn get_num_sectors(offset: i32) -> usize {
    (offset & 0xFF) as usize
}

fn get_sector_number(offset: i32) -> usize {
    (offset >> 8 & 0xFF_FFFF) as usize
}

fn size_to_sectors(size: usize) -> usize {
    size.div_ceil(SECTOR_BYTES)
}

fn get_offset_index(pos: &ChunkPos) -> usize {
    (pos.get_region_local_x() + pos.get_region_local_z() * 32) as usize
}

/// Reads until `buf` is full or EOF; returns the bytes read (`FileChannel.read(buf, position)`).
fn read_at_most(file: &File, buf: &mut [u8], position: u64) -> io::Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        match file.read_at(&mut buf[total..], position + total as u64) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(total)
}

fn temp_file_in(dir: &Path) -> io::Result<(PathBuf, File)> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    loop {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!("tmp{}.tmp", nanos ^ n.rotate_left(40) ^ (u64::from(std::process::id()) << 20)));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(f) => return Ok((path, f)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
}

impl RegionFile {
    /// `RegionFile(info, path, externalFileDir, sync)` with the globally selected compression.
    pub fn new(info: RegionStorageInfo, path: &Path, external_file_dir: &Path, sync: bool) -> io::Result<Self> {
        Self::with_version(info, path, external_file_dir, RegionFileVersion::get_selected(), sync)
    }

    /// `sync` opens the file with `O_DSYNC` (`StandardOpenOption.DSYNC`).
    pub fn with_version(
        info: RegionStorageInfo,
        path: &Path,
        external_file_dir: &Path,
        version: &'static RegionFileVersion,
        sync: bool,
    ) -> io::Result<Self> {
        if !external_file_dir.is_dir() {
            let abs = std::path::absolute(external_file_dir).unwrap_or_else(|_| external_file_dir.to_path_buf());
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("Expected directory, got {}", abs.display())));
        }
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true);
        if sync {
            options.custom_flags(libc::O_DSYNC);
        }
        let file = options.open(path)?;
        let mut region = Self {
            info,
            path: path.to_path_buf(),
            file,
            external_file_dir: external_file_dir.to_path_buf(),
            version,
            offsets: Box::new([0; SECTOR_INTS]),
            timestamps: Box::new([0; SECTOR_INTS]),
            used_sectors: RegionBitmap::new(),
        };
        region.used_sectors.force(0, 2);
        let mut header = [0u8; HEADER_SIZE];
        let read_header_bytes = read_at_most(&region.file, &mut header, 0)?;
        if read_header_bytes != 0 {
            if read_header_bytes != HEADER_SIZE {
                log::warn!("Region file {} has truncated header: {}", path.display(), read_header_bytes);
            }
            let size = fs::metadata(path)?.len();
            for i in 0..SECTOR_INTS {
                region.offsets[i] = i32::from_be_bytes(header[i * 4..i * 4 + 4].try_into().unwrap());
                region.timestamps[i] = i32::from_be_bytes(header[4096 + i * 4..4096 + i * 4 + 4].try_into().unwrap());
            }
            for i in 0..SECTOR_INTS {
                let offset = region.offsets[i];
                if offset != 0 {
                    let sector_number = get_sector_number(offset);
                    let num_sectors = get_num_sectors(offset);
                    if sector_number < 2 {
                        log::warn!(
                            "Region file {} has invalid sector at index: {}; sector {} overlaps with header",
                            path.display(),
                            i,
                            sector_number
                        );
                        region.offsets[i] = 0;
                    } else if num_sectors == 0 {
                        log::warn!("Region file {} has an invalid sector at index: {}; size has to be > 0", path.display(), i);
                        region.offsets[i] = 0;
                    } else if sector_number as u64 * SECTOR_BYTES as u64 > size {
                        log::warn!(
                            "Region file {} has an invalid sector at index: {}; sector {} is out of bounds",
                            path.display(),
                            i,
                            sector_number
                        );
                        region.offsets[i] = 0;
                    } else {
                        region.used_sectors.force(sector_number, num_sectors);
                    }
                }
            }
        }
        Ok(region)
    }

    pub fn get_path(&self) -> &Path {
        &self.path
    }

    pub fn used_sectors(&self) -> &RegionBitmap {
        &self.used_sectors
    }

    pub fn get_version(&self) -> &'static RegionFileVersion {
        self.version
    }

    /// Raw offset-table entry of `pos` (`sector_number << 8 | num_sectors`).
    pub fn get_offset(&self, pos: &ChunkPos) -> i32 {
        self.offsets[get_offset_index(pos)]
    }

    /// Timestamp-table entry (epoch seconds) of `pos`.
    pub fn get_timestamp(&self, pos: &ChunkPos) -> i32 {
        self.timestamps[get_offset_index(pos)]
    }

    fn get_external_chunk_path(&self, pos: &ChunkPos) -> PathBuf {
        self.external_file_dir.join(format!("c.{}.{}.mcc", pos.x, pos.z))
    }

    /// `getChunkDataInputStream`: decompressing reader over the chunk payload; `None` where vanilla
    /// returns null (missing, truncated, unsupported or invalid stream; logged like vanilla).
    pub fn get_chunk_data_input_stream(&self, pos: &ChunkPos) -> io::Result<Option<ChunkInputStream>> {
        let offset = self.get_offset(pos);
        if offset == 0 {
            return Ok(None);
        }
        let sector_number = get_sector_number(offset);
        let num_sectors = get_num_sectors(offset);
        let sectors_length = num_sectors * SECTOR_BYTES;
        let mut buffer = vec![0u8; sectors_length];
        let read = read_at_most(&self.file, &mut buffer, sector_number as u64 * SECTOR_BYTES as u64)?;
        buffer.truncate(read);
        if buffer.len() < CHUNK_HEADER_SIZE {
            log::error!("Chunk {} header is truncated: expected {} but read {}", pos, sectors_length, buffer.len());
            return Ok(None);
        }
        let length = i32::from_be_bytes(buffer[..4].try_into().unwrap());
        let version_id = buffer[4];
        if length == 0 {
            log::warn!("Chunk {} is allocated, but stream is missing", pos);
            return Ok(None);
        }
        let stream_length = length as i64 - 1;
        let remaining = (buffer.len() - CHUNK_HEADER_SIZE) as i64;
        if is_external_stream_chunk(version_id) {
            if stream_length != 0 {
                log::warn!("Chunk has both internal and external streams");
            }
            self.create_external_chunk_input_stream(pos, get_external_chunk_version(version_id))
        } else if stream_length > remaining {
            log::error!("Chunk {} stream is truncated: expected {} but read {}", pos, stream_length, remaining);
            Ok(None)
        } else if stream_length < 0 {
            log::error!("Declared size {} of chunk {} is negative", length, pos);
            Ok(None)
        } else {
            let payload = buffer[CHUNK_HEADER_SIZE..CHUNK_HEADER_SIZE + stream_length as usize].to_vec();
            Self::create_chunk_input_stream(pos, version_id, payload)
        }
    }

    fn create_chunk_input_stream(pos: &ChunkPos, version_id: u8, chunk_stream: Vec<u8>) -> io::Result<Option<ChunkInputStream>> {
        // Java bytes are signed: ids >= 128 never match a version.
        let version = RegionFileVersion::from_id(version_id as i8 as i32);
        match version {
            Some(v) if v == &VERSION_CUSTOM => {
                // DataInputStream.readUTF on the raw stream.
                let mut cursor = chunk_stream.as_slice();
                let mut len = [0u8; 2];
                cursor.read_exact(&mut len)?;
                let mut text = vec![0u8; u16::from_be_bytes(len) as usize];
                cursor.read_exact(&mut text)?;
                let text = String::from_utf8_lossy(&text);
                if let Some(id) = Identifier::try_parse(&text) {
                    log::error!("Unrecognized custom compression {}", id);
                } else {
                    log::error!("Invalid custom compression id {}", text);
                }
                Ok(None)
            }
            None => {
                log::error!("Chunk {} has invalid chunk stream version {}", pos, version_id as i8);
                Ok(None)
            }
            Some(v) => v.wrap_input(chunk_stream).map(Some),
        }
    }

    fn create_external_chunk_input_stream(&self, pos: &ChunkPos, version_id: u8) -> io::Result<Option<ChunkInputStream>> {
        let external_file = self.get_external_chunk_path(pos);
        if !external_file.is_file() {
            log::error!("External chunk path {} is not file", external_file.display());
            return Ok(None);
        }
        Self::create_chunk_input_stream(pos, version_id, fs::read(&external_file)?)
    }

    /// `doesChunkExist`: header sanity of the stored chunk without decoding it.
    pub fn does_chunk_exist(&self, pos: &ChunkPos) -> bool {
        let offset = self.get_offset(pos);
        if offset == 0 {
            return false;
        }
        let sector_number = get_sector_number(offset);
        let num_sectors = get_num_sectors(offset);
        let mut stream_header = [0u8; CHUNK_HEADER_SIZE];
        match read_at_most(&self.file, &mut stream_header, sector_number as u64 * SECTOR_BYTES as u64) {
            Ok(CHUNK_HEADER_SIZE) => {}
            _ => return false,
        }
        let length = i32::from_be_bytes(stream_header[..4].try_into().unwrap());
        let version_id = stream_header[4];
        if is_external_stream_chunk(version_id) {
            RegionFileVersion::is_valid_version(get_external_chunk_version(version_id) as i8 as i32)
                && self.get_external_chunk_path(pos).is_file()
        } else {
            if !RegionFileVersion::is_valid_version(version_id as i8 as i32) || length == 0 {
                return false;
            }
            let stream_length = length as i64 - 1;
            !(stream_length < 0 || stream_length > (SECTOR_BYTES * num_sectors) as i64)
        }
    }

    pub fn has_chunk(&self, pos: &ChunkPos) -> bool {
        self.get_offset(pos) != 0
    }

    /// `getChunkDataOutputStream(pos)` + write + close: compresses the uncompressed NBT payload
    /// with this file's version and stores it.
    pub fn write_chunk_data(&mut self, pos: &ChunkPos, raw: &[u8]) -> io::Result<()> {
        let compressed = self.version.compress(raw)?;
        let mut data = Vec::with_capacity(CHUNK_HEADER_SIZE + compressed.len());
        data.extend_from_slice(&[0; 4]);
        data.push(self.version.get_id() as u8);
        data.extend_from_slice(&compressed);
        // ChunkBuffer.close(): streamLength = count - 5 + 1
        let stream_length = (data.len() - CHUNK_HEADER_SIZE + 1) as i32;
        data[..4].copy_from_slice(&stream_length.to_be_bytes());
        self.write(pos, &data)
    }

    pub fn flush(&self) -> io::Result<()> {
        self.file.sync_all()
    }

    pub fn clear(&mut self, pos: &ChunkPos) -> io::Result<()> {
        let offset_index = get_offset_index(pos);
        let offset = self.offsets[offset_index];
        if offset != 0 {
            self.offsets[offset_index] = 0;
            self.timestamps[offset_index] = get_timestamp();
            self.write_header()?;
            match fs::remove_file(self.get_external_chunk_path(pos)) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
            self.used_sectors.free(get_sector_number(offset), get_num_sectors(offset));
        }
        Ok(())
    }

    /// `write(pos, ByteBuffer)`: `data` is the complete on-disk record
    /// (`length:i32be | id | payload`, length already filled in).
    pub fn write(&mut self, pos: &ChunkPos, data: &[u8]) -> io::Result<()> {
        let offset_index = get_offset_index(pos);
        let offset = self.offsets[offset_index];
        let sector_number = get_sector_number(offset);
        let current_sector_count = get_num_sectors(offset);
        let data_size = data.len();
        let mut sectors_needed = size_to_sectors(data_size);
        let new_sector_number;
        let mut external_commit: Option<(PathBuf, PathBuf)> = None;
        if sectors_needed >= EXTERNAL_CHUNK_THRESHOLD {
            let external_chunk_path = self.get_external_chunk_path(pos);
            log::warn!("Saving oversized chunk {} ({} bytes}} to external file {}", pos, data_size, external_chunk_path.display());
            sectors_needed = 1;
            new_sector_number = self.used_sectors.allocate(sectors_needed);
            let tmp_path = self.write_to_external_file(data)?;
            external_commit = Some((tmp_path, external_chunk_path));
            let mut stub = [0u8; CHUNK_HEADER_SIZE];
            stub[..4].copy_from_slice(&1i32.to_be_bytes());
            stub[4] = self.version.get_id() as u8 | EXTERNAL_STREAM_FLAG;
            self.file.write_all_at(&stub, new_sector_number as u64 * SECTOR_BYTES as u64)?;
        } else {
            new_sector_number = self.used_sectors.allocate(sectors_needed);
            self.file.write_all_at(data, new_sector_number as u64 * SECTOR_BYTES as u64)?;
        }
        self.offsets[offset_index] = pack_sector_offset(new_sector_number, sectors_needed);
        self.timestamps[offset_index] = get_timestamp();
        self.write_header()?;
        match external_commit {
            Some((tmp, target)) => fs::rename(tmp, target)?,
            None => match fs::remove_file(self.get_external_chunk_path(pos)) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            },
        }
        if sector_number != 0 {
            self.used_sectors.free(sector_number, current_sector_count);
        }
        Ok(())
    }

    /// `writeToExternalFile`: payload without the 5-byte header into a temp file; returns its path.
    fn write_to_external_file(&self, data: &[u8]) -> io::Result<PathBuf> {
        let (tmp_path, file) = temp_file_in(&self.external_file_dir)?;
        file.write_all_at(&data[CHUNK_HEADER_SIZE..], 0)?;
        Ok(tmp_path)
    }

    fn write_header(&self) -> io::Result<()> {
        let mut header = [0u8; HEADER_SIZE];
        for i in 0..SECTOR_INTS {
            header[i * 4..i * 4 + 4].copy_from_slice(&self.offsets[i].to_be_bytes());
            header[4096 + i * 4..4096 + i * 4 + 4].copy_from_slice(&self.timestamps[i].to_be_bytes());
        }
        self.file.write_all_at(&header, 0)
    }

    /// `close()`: pads the file to a whole sector, forces it to disk, closes it.
    pub fn close(self) -> io::Result<()> {
        let padded = self.pad_to_full_sector();
        let synced = self.file.sync_all();
        padded.and(synced)
    }

    fn pad_to_full_sector(&self) -> io::Result<()> {
        let file_size = self.file.metadata()?.len();
        let padded_size = size_to_sectors(file_size as usize) as u64 * SECTOR_BYTES as u64;
        if file_size != padded_size {
            self.file.write_all_at(&[0], padded_size - 1)?;
        }
        Ok(())
    }
}
