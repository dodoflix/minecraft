//! Path-based `NbtIo` entry points (`read(Path)`, `write(tag, Path)`, `readCompressed(Path, ..)`,
//! `writeCompressed(tag, Path)`).

use crate::error::Result;
use crate::{compress, io, CompoundTag, NbtAccounter};
use std::io::Write;
use std::path::Path;

/// `NbtIo.read(Path)`: uncompressed named compound; `None` when the file does not exist.
pub fn read_file(path: &Path) -> Result<Option<CompoundTag>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path)?;
    io::read(&mut bytes.as_slice(), &mut NbtAccounter::unlimited_heap()).map(Some)
}

/// `NbtIo.readCompressed(Path, accounter)`: gzip-wrapped named compound.
pub fn read_compressed_file(path: &Path, acc: &mut NbtAccounter) -> Result<CompoundTag> {
    compress::read_compressed(&std::fs::read(path)?, acc)
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
