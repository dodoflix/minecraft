//! `net.minecraft.util.worldupdate.UpgradeProgress`: the counters and status shared by the file fixer, the world
//! upgrader and whoever displays them, and the `world/upgrade_*` notifications.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use mc_core::ident::Identifier;
use mc_datafix::DataFixTypes;

use crate::level_storage::epoch_millis;

/// The `NotificationService` calls the upgrade makes (the JSON-RPC server maps them onto
/// `minecraft:notification/world/upgrade_*`).
pub trait UpgradeNotifications: Send + Sync {
    fn world_upgrade_started(&self) {}
    fn world_upgrade_progress(&self, _progress_percentage: f32) {}
    fn world_upgrade_finished(&self) {}
    fn world_upgrade_failed(&self, _reason: &str) {}
}

/// `EmptyNotificationService`.
pub struct NoNotifications;

impl UpgradeNotifications for NoNotifications {}

/// `UpgradeProgress.Status`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Counting,
    Failed,
    Finished,
    Upgrading,
}

/// `UpgradeProgress.Type`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Type {
    Files,
    LegacyStructures,
    Regions,
}

impl Type {
    /// `getSerializedName`.
    pub fn serialized_name(self) -> &'static str {
        match self {
            Type::Files => "files",
            Type::LegacyStructures => "legacy_structures",
            Type::Regions => "regions",
        }
    }
}

/// `UpgradeProgress.FileFixStats`.
#[derive(Default)]
pub struct FileFixStats {
    finished_operations: AtomicI32,
    total_operations: AtomicI32,
}

impl FileFixStats {
    pub fn finished_operations(&self) -> i32 {
        self.finished_operations.load(Ordering::Relaxed)
    }

    pub fn total_operations(&self) -> i32 {
        self.total_operations.load(Ordering::Relaxed)
    }

    pub fn reset(&self) {
        self.finished_operations.store(0, Ordering::Relaxed);
        self.total_operations.store(0, Ordering::Relaxed);
    }

    /// `getProgress`: finished / total clamped to `[0, 1]`, `0` without operations.
    pub fn progress(&self) -> f32 {
        if self.total_operations() == 0 {
            0.0
        } else {
            (self.finished_operations() as f32 / self.total_operations() as f32).clamp(0.0, 1.0)
        }
    }
}

pub struct UpgradeProgress {
    /// `UpgradeProgress.Noop`: every mutator the region upgrader calls does nothing.
    noop: bool,
    finished: AtomicBool,
    total_file_fix_stats: FileFixStats,
    type_file_fix_stats: FileFixStats,
    running_file_fixer_stats: FileFixStats,
    total_progress: AtomicU32,
    total_chunks: AtomicI32,
    converted: AtomicI32,
    skipped: AtomicI32,
    dimension_progress: Mutex<HashMap<Identifier, f32>>,
    canceled: AtomicBool,
    data_fix_type: Mutex<Option<DataFixTypes>>,
    status: Mutex<Status>,
    type_: Mutex<Option<Type>>,
    last_logged_progress_time: AtomicI64,
    notifications: Arc<dyn UpgradeNotifications>,
}

impl Default for UpgradeProgress {
    fn default() -> Self {
        Self::new(Arc::new(NoNotifications))
    }
}

impl UpgradeProgress {
    pub fn new(notifications: Arc<dyn UpgradeNotifications>) -> UpgradeProgress {
        UpgradeProgress {
            noop: false,
            finished: AtomicBool::new(false),
            total_file_fix_stats: FileFixStats::default(),
            type_file_fix_stats: FileFixStats::default(),
            running_file_fixer_stats: FileFixStats::default(),
            total_progress: AtomicU32::new(0f32.to_bits()),
            total_chunks: AtomicI32::new(0),
            converted: AtomicI32::new(0),
            skipped: AtomicI32::new(0),
            dimension_progress: Mutex::new(HashMap::new()),
            canceled: AtomicBool::new(false),
            data_fix_type: Mutex::new(None),
            status: Mutex::new(Status::Counting),
            type_: Mutex::new(None),
            last_logged_progress_time: AtomicI64::new(0),
            notifications,
        }
    }

    /// `new UpgradeProgress.Noop()`.
    pub fn noop() -> UpgradeProgress {
        UpgradeProgress {
            noop: true,
            ..Self::default()
        }
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Relaxed)
    }

    pub fn set_finished(&self, finished: bool) {
        if !self.noop {
            self.finished.store(finished, Ordering::Relaxed);
        }
    }

    pub fn total_file_fix_stats(&self) -> &FileFixStats {
        &self.total_file_fix_stats
    }

    pub fn type_file_fix_stats(&self) -> &FileFixStats {
        &self.type_file_fix_stats
    }

    pub fn running_file_fixer_stats(&self) -> &FileFixStats {
        &self.running_file_fixer_stats
    }

    pub fn add_total_file_fix_operations(&self, additional: i32) {
        if !self.noop {
            self.total_file_fix_stats
                .total_operations
                .fetch_add(additional, Ordering::Relaxed);
            self.type_file_fix_stats
                .total_operations
                .fetch_add(additional, Ordering::Relaxed);
        }
    }

    pub fn total_progress(&self) -> f32 {
        f32::from_bits(self.total_progress.load(Ordering::Relaxed))
    }

    pub fn set_total_progress(&self, total_progress: f32) {
        if !self.noop {
            self.total_progress
                .store(total_progress.to_bits(), Ordering::Relaxed);
        }
    }

    pub fn total_chunks(&self) -> i32 {
        self.total_chunks.load(Ordering::Relaxed)
    }

    pub fn add_total_chunks(&self, additional: i32) {
        if !self.noop {
            self.total_chunks.fetch_add(additional, Ordering::Relaxed);
        }
    }

    pub fn converted(&self) -> i32 {
        self.converted.load(Ordering::Relaxed)
    }

    pub fn increment_converted(&self) {
        if !self.noop {
            self.converted.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn skipped(&self) -> i32 {
        self.skipped.load(Ordering::Relaxed)
    }

    pub fn increment_skipped(&self) {
        if !self.noop {
            self.skipped.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn set_dimension_progress(&self, dimension: Identifier, current_progress: f32) {
        if !self.noop {
            self.dimension_progress
                .lock()
                .unwrap()
                .insert(dimension, current_progress);
        }
    }

    /// `getDimensionProgress`: `0` for a dimension without progress (`Reference2FloatMap.defaultReturnValue`).
    pub fn dimension_progress(&self, dimension: &Identifier) -> f32 {
        self.dimension_progress
            .lock()
            .unwrap()
            .get(dimension)
            .copied()
            .unwrap_or(0.0)
    }

    pub fn increment_finished_operations(&self) {
        self.increment_finished_operations_by(1);
    }

    pub fn increment_finished_operations_by(&self, count: i32) {
        if self.noop {
            return;
        }
        self.total_file_fix_stats
            .finished_operations
            .fetch_add(count, Ordering::Relaxed);
        self.type_file_fix_stats
            .finished_operations
            .fetch_add(count, Ordering::Relaxed);
        self.log_progress();
    }

    pub fn set_canceled(&self) {
        if !self.noop {
            self.canceled.store(true, Ordering::Relaxed);
        }
    }

    pub fn is_canceled(&self) -> bool {
        self.canceled.load(Ordering::Relaxed)
    }

    pub fn status(&self) -> Status {
        *self.status.lock().unwrap()
    }

    pub fn set_status(&self, status: Status) {
        if !self.noop {
            *self.status.lock().unwrap() = status;
        }
    }

    pub fn data_fix_type(&self) -> Option<DataFixTypes> {
        *self.data_fix_type.lock().unwrap()
    }

    /// `setType`: also resets the per-type stats.
    pub fn set_type(&self, type_: Type) {
        *self.type_.lock().unwrap() = Some(type_);
        self.type_file_fix_stats.reset();
    }

    pub fn type_(&self) -> Option<Type> {
        *self.type_.lock().unwrap()
    }

    pub fn set_applicable_fixer_amount(&self, amount: i32) {
        if !self.noop {
            self.running_file_fixer_stats
                .total_operations
                .store(amount, Ordering::Relaxed);
        }
    }

    pub fn increment_running_file_fixer(&self) {
        if !self.noop {
            self.running_file_fixer_stats
                .finished_operations
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    /// `reset(dataFixType)`.
    pub fn reset(&self, data_fix_type: DataFixTypes) {
        if self.noop {
            return;
        }
        self.total_file_fix_stats.reset();
        self.type_file_fix_stats.reset();
        self.total_chunks.store(0, Ordering::Relaxed);
        self.converted.store(0, Ordering::Relaxed);
        self.skipped.store(0, Ordering::Relaxed);
        *self.data_fix_type.lock().unwrap() = Some(data_fix_type);
    }

    /// `logProgress`: at most once a second, `Upgrading progress: <n>%`.
    pub fn log_progress(&self) {
        // `Util.getMillis()` has an arbitrary origin; the wall clock does for a once-a-second throttle.
        let now = epoch_millis();
        if now >= self.last_logged_progress_time.load(Ordering::Relaxed) + 1000 {
            let progress = self.total_file_fix_stats.finished_operations() as f32
                / self.total_file_fix_stats.total_operations() as f32;
            self.last_logged_progress_time.store(now, Ordering::Relaxed);
            // `(int) (progress * 100.0F)`: saturating, NaN (0 / 0) is 0.
            log::info!("Upgrading progress: {}%", (progress * 100.0) as i32);
            self.notifications.world_upgrade_progress(progress);
        }
    }

    pub fn notifications(&self) -> &dyn UpgradeNotifications {
        &*self.notifications
    }
}
