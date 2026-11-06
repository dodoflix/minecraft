//! The vanilla log4j2 setup (`com.mojang.logging` `log4j2.xml`) behind the `log` crate: every record goes to stdout and
//! `logs/latest.log` as `[HH:mm:ss] [<thread>/<LEVEL>]: <message>`; `latest.log` is rolled to
//! `logs/yyyy-MM-dd-N.log.gz` on startup (and at midnight), keeping 7 files per day like log4j's default strategy.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::{DateTime, Local, NaiveDate};
use flate2::Compression;
use flate2::write::GzEncoder;
use log::{LevelFilter, Log, Metadata, Record};
use parking_lot::Mutex;

/// `DefaultRolloverStrategy` default `max`.
const MAX_ROLLED_FILES: u32 = 7;

/// `%d{HH:mm:ss} ... %t/%level` line, one record, including the trailing newline.
pub fn format_line(
    time: &DateTime<Local>,
    thread: &str,
    level: log::Level,
    message: &str,
) -> String {
    format!(
        "[{}] [{}/{}]: {}\n",
        time.format("%H:%M:%S"),
        thread,
        level_name(level),
        message
    )
}

fn level_name(level: log::Level) -> &'static str {
    match level {
        log::Level::Error => "ERROR",
        log::Level::Warn => "WARN",
        log::Level::Info => "INFO",
        log::Level::Debug => "DEBUG",
        log::Level::Trace => "TRACE",
    }
}

/// Java gives unnamed threads `Thread-N`.
fn current_thread_name() -> String {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    thread_local! {
        static UNNAMED: usize = NEXT.fetch_add(1, Ordering::Relaxed);
    }
    match std::thread::current().name() {
        Some(name) => name.to_owned(),
        None => format!("Thread-{}", UNNAMED.with(|n| *n)),
    }
}

fn rolled_name(date: NaiveDate, index: u32) -> String {
    format!("{}-{index}.log.gz", date.format("%Y-%m-%d"))
}

/// `OnStartupTriggeringPolicy` + `DefaultRolloverStrategy`: gzip `latest.log` (if it holds anything) to the next free
/// `<date>-<n>.log.gz` where `date` is the file's last modification day. With all 7 slots taken the oldest is
/// deleted and the rest shift down, so the newest always ends up at index 7.
pub fn roll_latest_log(dir: &Path, date_override: Option<NaiveDate>) -> io::Result<()> {
    let latest = dir.join("latest.log");
    let meta = match fs::metadata(&latest) {
        Ok(m) if m.len() > 0 => m,
        _ => return Ok(()),
    };
    let date = match date_override {
        Some(d) => d,
        None => DateTime::<Local>::from(meta.modified()?).date_naive(),
    };
    let existing: Vec<u32> = (1..=MAX_ROLLED_FILES)
        .filter(|i| dir.join(rolled_name(date, *i)).exists())
        .collect();
    let target = match existing.last() {
        None => 1,
        Some(&highest) if highest < MAX_ROLLED_FILES => highest + 1,
        Some(_) => {
            let lowest = existing[0];
            fs::remove_file(dir.join(rolled_name(date, lowest)))?;
            for &i in existing.iter().filter(|&&i| i != lowest) {
                fs::rename(
                    dir.join(rolled_name(date, i)),
                    dir.join(rolled_name(date, i - 1)),
                )?;
            }
            MAX_ROLLED_FILES
        }
    };
    let mut encoder = GzEncoder::new(
        File::create(dir.join(rolled_name(date, target)))?,
        Compression::default(),
    );
    io::copy(&mut File::open(&latest)?, &mut encoder)?;
    encoder.finish()?;
    fs::remove_file(&latest)
}

struct Sink {
    dir: PathBuf,
    file: File,
    day: NaiveDate,
}

struct VanillaLogger {
    level: LevelFilter,
    sink: Mutex<Sink>,
}

impl Log for VanillaLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let now = Local::now();
        let line = format_line(
            &now,
            &current_thread_name(),
            record.level(),
            &record.args().to_string(),
        );
        let mut sink = self.sink.lock();
        if now.date_naive() != sink.day {
            // `TimeBasedTriggeringPolicy`: a new day starts a new file
            let previous = sink.day;
            if roll_latest_log(&sink.dir, Some(previous)).is_ok()
                && let Ok(file) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(sink.dir.join("latest.log"))
            {
                sink.file = file;
            }
            sink.day = now.date_naive();
        }
        let stdout = io::stdout();
        let mut out = stdout.lock();
        let _ = out.write_all(line.as_bytes());
        let _ = out.flush();
        let _ = sink.file.write_all(line.as_bytes());
    }

    fn flush(&self) {
        let _ = io::stdout().flush();
    }
}

/// Install the logger: roll the previous `latest.log`, open a fresh one in `logs_dir`, log everything at or above
/// `level` (vanilla: `INFO`). Fails when the directory or file cannot be created, or a logger is already set.
pub fn init(logs_dir: impl AsRef<Path>, level: LevelFilter) -> io::Result<()> {
    let dir = logs_dir.as_ref().to_owned();
    fs::create_dir_all(&dir)?;
    roll_latest_log(&dir, None)?;
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("latest.log"))?;
    let logger = VanillaLogger {
        level,
        sink: Mutex::new(Sink {
            dir,
            file,
            day: Local::now().date_naive(),
        }),
    };
    log::set_logger(Box::leak(Box::new(logger))).map_err(|e| io::Error::other(e.to_string()))?;
    log::set_max_level(level);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::io::Read;

    #[test]
    fn line_format() {
        let t = Local.with_ymd_and_hms(2026, 11, 4, 9, 5, 7).unwrap();
        assert_eq!(
            format_line(
                &t,
                "Server thread",
                log::Level::Info,
                "Done (1.2s)! For help, type \"help\""
            ),
            "[09:05:07] [Server thread/INFO]: Done (1.2s)! For help, type \"help\"\n"
        );
        assert_eq!(
            format_line(&t, "Worker-Main-1", log::Level::Warn, "x"),
            "[09:05:07] [Worker-Main-1/WARN]: x\n"
        );
    }

    fn gunzip(path: &Path) -> String {
        let mut s = String::new();
        flate2::read::GzDecoder::new(File::open(path).unwrap())
            .read_to_string(&mut s)
            .unwrap();
        s
    }

    #[test]
    fn rotation_numbers_and_shifts() {
        let dir = tempfile::tempdir().unwrap();
        let d = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
        for run in 1..=9 {
            fs::write(dir.path().join("latest.log"), format!("run {run}\n")).unwrap();
            roll_latest_log(dir.path(), Some(d)).unwrap();
            assert!(!dir.path().join("latest.log").exists());
        }
        // 9 rolls with 7 slots: the newest is index 7, the oldest kept is run 3
        for i in 1..=7 {
            assert_eq!(
                gunzip(&dir.path().join(format!("2026-11-04-{i}.log.gz"))),
                format!("run {}\n", i + 2)
            );
        }
        // empty / missing latest.log is left alone
        roll_latest_log(dir.path(), Some(d)).unwrap();
        fs::write(dir.path().join("latest.log"), "").unwrap();
        roll_latest_log(dir.path(), Some(d)).unwrap();
        assert!(dir.path().join("latest.log").exists());
    }
}
