//! `server.dedicated.ServerWatchdog`: a daemon thread that declares the server crashed when one tick takes longer
//! than `max-tick-time`.
//!
//! Deviation from vanilla: Java stack traces of other threads do not exist; the "Thread Dump" category lists
//! the process' threads with their kernel state from `/proc/self/task` instead.

use super::shared::{ServerShared, get_nanos};
use crate::crash_report::{CrashReport, filename_formatted_date_time};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const NANOS_PER_MILLISECOND: i64 = 1_000_000;
const NANOS_PER_SECOND: i64 = 1_000_000_000;
/// `ServerWatchdog.MAX_SHUTDOWN_TIME`.
const MAX_SHUTDOWN_TIME: Duration = Duration::from_millis(10_000);

/// What the game thread publishes for the watchdog's report (refreshed regularly; the watchdog cannot touch
/// game-thread state while the game thread is stuck).
#[derive(Default, Clone)]
pub struct WatchdogInfo {
    /// `TickRateManager.millisecondsPerTick`.
    pub milliseconds_per_tick: f32,
    /// `System Details` entries of `MinecraftServer.fillSystemReport`.
    pub system_details: Vec<(String, String)>,
    /// `Performance stats` entries (`Random tick rate`, `Level stats`).
    pub performance_stats: Vec<(String, String)>,
}

pub type SharedWatchdogInfo = Arc<Mutex<WatchdogInfo>>;

/// `new Thread(new ServerWatchdog(server), "Server Watchdog")` (daemon).
pub fn spawn(
    shared: Arc<ServerShared>,
    info: SharedWatchdogInfo,
    max_tick_time_ms: i64,
    server_directory: PathBuf,
) -> std::io::Result<()> {
    let max_tick_time_nanos = max_tick_time_ms.wrapping_mul(NANOS_PER_MILLISECOND);
    std::thread::Builder::new()
        .name("Server Watchdog".to_owned())
        .spawn(move || run(&shared, &info, max_tick_time_nanos, &server_directory))?;
    Ok(())
}

fn run(
    shared: &ServerShared,
    info: &Mutex<WatchdogInfo>,
    max_tick_time_nanos: i64,
    server_directory: &std::path::Path,
) {
    while shared.is_running() {
        let next_tick_time_nanos = shared.next_tick_time_nanos();
        let current_time_nanos = get_nanos();
        let delta_nanos = current_time_nanos - next_tick_time_nanos;
        if delta_nanos > max_tick_time_nanos {
            let info = info.lock().clone();
            log::error!(
                "A single server tick took {:.2} seconds (should be max {:.2})",
                delta_nanos as f32 / NANOS_PER_SECOND as f32,
                info.milliseconds_per_tick / 1000.0
            );
            log::error!("Considering it to be crashed, server will forcibly shutdown.");
            let mut report = create_watchdog_crash_report("Watching Server");
            for (key, value) in &info.system_details {
                report.system_report().set_detail(key.clone(), value);
            }
            let stats = report.add_category("Performance stats");
            for (key, value) in &info.performance_stats {
                stats.set_detail(key.clone(), value);
            }
            // Bootstrap.realStdoutPrintln
            log::error!("Crash report:\n{}", report.get_friendly_report());
            let file = server_directory.join("crash-reports").join(format!(
                "crash-{}-server.txt",
                filename_formatted_date_time()
            ));
            if report.save_to_file(&file) {
                log::error!(
                    "This crash report has been saved to: {}",
                    std::path::absolute(&file).unwrap_or(file).display()
                );
            } else {
                log::error!("We were unable to save this crash report to disk.");
            }
            exit(shared);
        }
        let sleep_ms = (next_tick_time_nanos + max_tick_time_nanos - current_time_nanos)
            / NANOS_PER_MILLISECOND;
        std::thread::sleep(Duration::from_millis(sleep_ms.max(0) as u64));
    }
}

/// `ServerWatchdog.createWatchdogCrashReport`.
pub fn create_watchdog_crash_report(message: &str) -> CrashReport {
    let mut report = CrashReport::new(message, format!("java.lang.Error: Watchdog ({message})"));
    report
        .add_category("Thread Dump")
        .set_detail("Threads", thread_dump());
    report
}

/// `Util.dumpThreadInfo` replacement: one line per thread (`name`, tid, kernel state).
fn thread_dump() -> String {
    let mut threads: Vec<(String, String, String)> = Vec::new();
    if let Ok(dir) = std::fs::read_dir("/proc/self/task") {
        for entry in dir.flatten() {
            let tid = entry.file_name().to_string_lossy().into_owned();
            let comm = std::fs::read_to_string(entry.path().join("comm"))
                .unwrap_or_default()
                .trim_end()
                .to_owned();
            let state = std::fs::read_to_string(entry.path().join("stat"))
                .ok()
                .and_then(|s| {
                    s.rsplit_once(')')
                        .and_then(|(_, rest)| rest.split_whitespace().next().map(str::to_owned))
                })
                .unwrap_or_else(|| "?".to_owned());
            threads.push((comm, tid, state));
        }
    }
    threads.sort();
    let mut out = String::new();
    for (name, tid, state) in threads {
        out.push_str(&format!("\n\"{name}\" tid={tid} state={state}"));
    }
    out
}

/// `ServerWatchdog.exit`: ask for a normal shutdown (vanilla: `System.exit(1)` runs the shutdown hook that halts
/// the server), and kill the process after `MAX_SHUTDOWN_TIME` when the stuck game thread does not finish.
fn exit(shared: &ServerShared) {
    shared.halt();
    let deadline = std::time::Instant::now() + MAX_SHUTDOWN_TIME;
    while !shared.is_shutdown() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_lists_current_thread() {
        let report = create_watchdog_crash_report("Watching Server");
        let text = report.get_friendly_report();
        assert!(text.contains("Description: Watching Server"));
        assert!(text.contains("-- Thread Dump --\nDetails:\n\tThreads: \n\""));
    }
}
