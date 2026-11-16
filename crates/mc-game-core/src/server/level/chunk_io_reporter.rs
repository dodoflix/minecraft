//! `MinecraftServer.reportChunkLoadFailure` / `reportChunkSaveFailure` / `storeChunkIoError` and
//! `ChunkIOErrorReporter.reportMisplacedChunk`: a failed chunk load or save is logged, written to
//! `debug/chunk-<level>-<time>-server.txt` and may raise the low disk space warning.

use crate::crash_report::{CrashReport, ReportType, filename_formatted_date_time};
use crate::util::file_util::sanitize_name;
use mc_core::pos::ChunkPos;
use mc_world::level_storage::usable_space;
use mc_world::region::RegionStorageInfo;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// Reports stay unwritten when the drive has less usable space than this.
const MIN_REPORT_SPACE: u64 = 8192;

/// The server's chunk IO error reporter, shared by every level (`MinecraftServer implements ChunkIOErrorReporter`).
#[derive(Default)]
pub struct ChunkIoReporter {
    /// `getFile("debug")`: unset until a server owns the reporter (levels outside a server only log).
    debug_dir: OnceLock<PathBuf>,
    /// A failure was reported: the server runs `warnOnLowDiskSpace` on its next tick (vanilla runs it right there, on
    /// whichever thread reported).
    low_disk_space_check: AtomicBool,
}

impl ChunkIoReporter {
    pub fn set_debug_dir(&self, dir: PathBuf) {
        let _ = self.debug_dir.set(dir);
    }

    /// Whether a failure asked for the low disk space check since the last call.
    pub fn take_low_disk_space_check(&self) -> bool {
        self.low_disk_space_check.swap(false, Ordering::AcqRel)
    }

    /// `reportChunkLoadFailure(throwable, storageInfo, pos)`.
    pub fn report_chunk_load_failure(
        &self,
        exception: &str,
        info: &RegionStorageInfo,
        pos: ChunkPos,
    ) {
        log::error!("Failed to load chunk {},{}\n{exception}", pos.x, pos.z);
        self.store_and_warn(CrashReport::new("Chunk load failure", exception), info, pos);
    }

    /// `reportChunkSaveFailure(throwable, storageInfo, pos)`.
    pub fn report_chunk_save_failure(
        &self,
        exception: &str,
        info: &RegionStorageInfo,
        pos: ChunkPos,
    ) {
        log::error!("Failed to save chunk {},{}\n{exception}", pos.x, pos.z);
        self.store_and_warn(CrashReport::new("Chunk save failure", exception), info, pos);
    }

    /// `reportMisplacedChunk(storedPos, requestedPos, storageInfo)`: a load failure whose report is the "Misplaced Chunk" one.
    pub fn report_misplaced_chunk(
        &self,
        stored_pos: ChunkPos,
        requested_pos: ChunkPos,
        info: &RegionStorageInfo,
    ) {
        let exception = format!(
            "java.lang.IllegalStateException: Retrieved chunk position {stored_pos} does not match requested {requested_pos}"
        );
        let mut report = CrashReport::new("Chunk found in invalid location", exception.clone());
        report
            .add_category("Misplaced Chunk")
            .set_detail("Stored Position", stored_pos);
        log::error!(
            "Failed to load chunk {},{}\n{exception}",
            requested_pos.x,
            requested_pos.z
        );
        self.store_and_warn(report, info, requested_pos);
    }

    /// `storeChunkIoError(report, pos, storageInfo)` on the IO pool, then `warnOnLowDiskSpace`.
    fn store_and_warn(&self, report: CrashReport, info: &RegionStorageInfo, pos: ChunkPos) {
        if let Some(debug_dir) = self.debug_dir.get() {
            let (debug_dir, info) = (debug_dir.clone(), info.clone());
            let spawned = std::thread::Builder::new()
                .name("IO-Worker".to_owned())
                .spawn(move || store_chunk_io_error(report, pos, &info, &debug_dir));
            if let Err(e) = spawned {
                log::warn!("Failed to store chunk IO exception\n{e}");
            }
        }
        self.low_disk_space_check.store(true, Ordering::Release);
    }
}

fn store_chunk_io_error(
    mut report: CrashReport,
    pos: ChunkPos,
    info: &RegionStorageInfo,
    debug_dir: &Path,
) {
    if let Err(e) = std::fs::create_dir_all(debug_dir) {
        log::warn!("Failed to store chunk IO exception\n{e}");
        return;
    }
    let report_file = debug_dir.join(format!(
        "chunk-{}-{}-server.txt",
        sanitize_name(&info.level),
        filename_formatted_date_time()
    ));
    if usable_space(debug_dir) < MIN_REPORT_SPACE {
        log::warn!(
            "Not storing chunk IO report due to low space on drive {}",
            debug_dir.display()
        );
        return;
    }
    report
        .add_category("Chunk Info")
        .set_detail("Level", &info.level)
        .set_detail("Dimension", &info.dimension)
        .set_detail("Storage", &info.r#type)
        .set_detail("Position", pos);
    if report.save_to_file(&report_file, ReportType::CHUNK_IO_ERROR) {
        log::info!(
            "Saved details to {}",
            report.save_file().unwrap_or(&report_file).display()
        );
    }
}
