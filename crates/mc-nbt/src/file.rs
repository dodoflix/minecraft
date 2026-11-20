//! Path-based `NbtIo` entry points (`read(Path)`, `write(tag, Path)`, `readCompressed(Path, ..)`,
//! `writeCompressed(tag, Path)`).

use crate::error::{NbtError, Result};
use crate::{CompoundTag, NbtAccounter, compress, io};
use mc_core::java_exception::JavaException;
use std::io::{Read, Write};
use std::path::Path;

/// `NbtIo.read(Path)`: uncompressed named compound; `None` when the file does not exist.
pub fn read_file(path: &Path) -> Result<Option<CompoundTag>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = read_bytes(path)?;
    io::read(&mut bytes.as_slice(), &mut NbtAccounter::unlimited_heap()).map(Some)
}

/// `NbtIo.readCompressed(Path, accounter)`: gzip-wrapped named compound.
pub fn read_compressed_file(path: &Path, acc: &mut NbtAccounter) -> Result<CompoundTag> {
    compress::read_compressed(&read_bytes(path)?, acc)
}

/// The bytes of the file, a failure with the exception `Files.newInputStream` and its reads throw.
fn read_bytes(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| JavaException::translate_file_error(&e, path))
        .and_then(|mut file| {
            file.read_to_end(&mut bytes)
                .map_err(|e| JavaException::read_failure(&e))
        })
        .map_err(NbtError::Stream)?;
    Ok(bytes)
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<()> {
    // SYNC | WRITE | CREATE | TRUNCATE_EXISTING
    let mut f = std::fs::File::create(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}

/// `NbtIo.write(CompoundTag, Path)`.
pub fn write_file(tag: &CompoundTag, path: &Path) -> Result<()> {
    write_synced(path, &io::to_bytes(tag))
}

/// `NbtIo.writeCompressed(CompoundTag, Path)`.
pub fn write_compressed_file(tag: &CompoundTag, path: &Path) -> Result<()> {
    write_synced(path, &compress::write_compressed(tag))
}
