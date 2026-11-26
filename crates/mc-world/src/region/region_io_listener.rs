//! Where `RegionFile` reports the chunk streams it reads and writes (`JvmProfiler.onRegionFileRead` and
//! `onRegionFileWrite`); the server installs its flight recorder here.

use std::sync::OnceLock;

use mc_core::pos::chunk_pos::ChunkPos;

use super::RegionStorageInfo;

pub trait RegionIoListener: Send + Sync {
    /// `streamLength` is the length of the chunk's stream without its version byte; `version_id` is the compression
    /// id of the region file (`RegionFileVersion.getId()`), not the one the chunk was stored with.
    fn on_region_file_read(
        &self,
        info: &RegionStorageInfo,
        pos: &ChunkPos,
        version_id: i32,
        stream_length: i32,
    );

    fn on_region_file_write(
        &self,
        info: &RegionStorageInfo,
        pos: &ChunkPos,
        version_id: i32,
        stream_length: i32,
    );
}

static LISTENER: OnceLock<&'static dyn RegionIoListener> = OnceLock::new();

/// Installs the listener; the first one wins.
pub fn set_listener(listener: &'static dyn RegionIoListener) {
    let _ = LISTENER.set(listener);
}

pub(super) fn listener() -> Option<&'static dyn RegionIoListener> {
    LISTENER.get().copied()
}
