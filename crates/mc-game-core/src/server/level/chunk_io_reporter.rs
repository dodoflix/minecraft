//! `MinecraftServer.reportChunkLoadFailure` / `reportChunkSaveFailure` / `storeChunkIoError` and
//! `ChunkIOErrorReporter.reportMisplacedChunk`: a failed chunk load or save is logged, written to
//! `debug/chunk-<level>-<time>-server.txt`, remembered as a suppressed exception and may raise the low disk space warning.

use crate::crash_report::{CrashReport, ReportType, filename_formatted_date_time};
use crate::server::suppressed_exceptions::SuppressedExceptionCollector;
use crate::util::file_util::sanitize_name;
use mc_core::pos::ChunkPos;
use mc_world::level_storage::usable_space;
use mc_world::region::RegionStorageInfo;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

/// Reports stay unwritten when the drive has less usable space than this.
const MIN_REPORT_SPACE: u64 = 8192;

/// The server's chunk IO error reporter, shared by every level (`MinecraftServer implements ChunkIOErrorReporter`).
pub struct ChunkIoReporter {
    suppressed: Arc<SuppressedExceptionCollector>,
    /// `getFile("debug")`: unset until a server owns the reporter (levels outside a server only log).
    debug_dir: OnceLock<PathBuf>,
    /// `storageSource.checkForLowDiskSpace()`: unset until a server owns the reporter.
    low_disk_space: OnceLock<Box<dyn Fn() -> bool + Send + Sync>>,
    /// The warning was logged by a reporting thread and the operators still have to be told (their connections belong to the
    /// game thread).
    ops_warning: AtomicBool,
}

#[cfg(test)]
impl Default for ChunkIoReporter {
    /// A reporter that owns its suppressed exceptions (levels outside a server).
    fn default() -> Self {
        Self::new(Arc::default())
    }
}

impl ChunkIoReporter {
    pub fn new(suppressed: Arc<SuppressedExceptionCollector>) -> Self {
        Self {
            suppressed,
            debug_dir: OnceLock::new(),
            low_disk_space: OnceLock::new(),
            ops_warning: AtomicBool::new(false),
        }
    }

    pub fn set_debug_dir(&self, dir: PathBuf) {
        let _ = self.debug_dir.set(dir);
    }

    /// What `warnOnLowDiskSpace` asks after every report.
    pub fn set_low_disk_space_check(&self, check: impl Fn() -> bool + Send + Sync + 'static) {
        let _ = self.low_disk_space.set(Box::new(check));
    }

    /// Whether a report found the disk low since the last call: the game thread then tells the operators.
    pub fn take_ops_warning(&self) -> bool {
        self.ops_warning.swap(false, Ordering::AcqRel)
    }

    /// `reportChunkLoadFailure(throwable, storageInfo, pos)`.
    pub fn report_chunk_load_failure(
        &self,
        exception: &str,
        info: &RegionStorageInfo,
        pos: ChunkPos,
    ) {
        log::error!("Failed to load chunk {},{}\n{exception}", pos.x, pos.z);
        self.suppress("chunk/load", exception);
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
        self.suppress("chunk/save", exception);
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
        // the exception handed to `reportChunkLoadFailure` is the `ReportedException` around the report
        self.suppressed.add_entry(
            "chunk/load",
            "net.minecraft.ReportedException",
            "Chunk found in invalid location",
        );
        self.store_and_warn(report, info, requested_pos);
    }

    /// `suppressedExceptions.addEntry(location, throwable)`: a Rust error stands for a `java.io.IOException`, the text of
    /// a Java exception (`java.lang.X: message`) keeps its class.
    fn suppress(&self, location: &str, exception: &str) {
        let (class, message) = match exception.split_once(": ") {
            Some((class, message)) if is_java_class_name(class) => (class, message),
            _ => ("java.io.IOException", exception),
        };
        self.suppressed.add_entry(location, class, message);
    }

    /// `storeChunkIoError(report, pos, storageInfo)` on the IO pool, then `warnOnLowDiskSpace` on the reporting thread.
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
        if self.low_disk_space.get().is_some_and(|check| check()) {
            log::warn!("Low disk space! Might not be able to save the world.");
            self.ops_warning.store(true, Ordering::Release);
        }
    }
}

/// `package.Class`: lower case package segments, then a capitalized class.
fn is_java_class_name(text: &str) -> bool {
    text.split_once('.').is_some()
        && text
            .split('.')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_alphanumeric() || c == '_'))
        && text
            .rsplit('.')
            .next()
            .is_some_and(|c| c.starts_with(char::is_uppercase))
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
