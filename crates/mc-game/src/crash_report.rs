//! `net.minecraft.CrashReport` / `CrashReportCategory` / `SystemReport` / `ReportType` / `ReportedException`.
//!
//! Java stack traces are replaced by the panic message plus a Rust backtrace captured by the hook that
//! [`install_panic_hook`] installs (the report text keeps vanilla's layout: header, `Time:`, `Description:`, the
//! exception, the "detailed walkthrough" line and the `-- Category --` / `-- System Details --` blocks).

use mc_world::level_storage::WORLD_VERSION_NAME;
use std::any::Any;
use std::cell::RefCell;
use std::fmt::Display;
use std::io::Write;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};

/// `ReportType.CRASH` header + nuggets (only the crash report is produced by the server lifecycle).
const CRASH_HEADER: &str = "Minecraft Crash Report";
const CRASH_NUGGETS: [&str; 34] = [
    "Who set us up the TNT?",
    "Everything's going to plan. No, really, that was supposed to happen.",
    "Uh... Did I do that?",
    "Oops.",
    "Why did you do that?",
    "I feel sad now :(",
    "My bad.",
    "I'm sorry, Dave.",
    "I let you down. Sorry :(",
    "On the bright side, I bought you a teddy bear!",
    "Daisy, daisy...",
    "Oh - I know what I did wrong!",
    "Hey, that tickles! Hehehe!",
    "I blame Dinnerbone.",
    "You should try our sister game, Minceraft!",
    "Don't be sad. I'll do better next time, I promise!",
    "Don't be sad, have a hug! <3",
    "I just don't know what went wrong :(",
    "Shall we play a game?",
    "Quite honestly, I wouldn't worry myself about that.",
    "I bet Cylons wouldn't have this problem.",
    "Sorry :(",
    "Surprise! Haha. Well, this is awkward.",
    "Would you like a cupcake?",
    "Hi. I'm Minecraft, and I'm a crashaholic.",
    "Ooh. Shiny.",
    "This doesn't make any sense!",
    "Why is it breaking :(",
    "Don't do that.",
    "Ouch. That hurt :(",
    "You're mean.",
    "This is a token for 1 free hug. Redeem at your nearest Mojangsta: [~~HUG~~]",
    "There are four lights!",
    "But it works on my machine.",
];

/// `CrashReportCategory`: a titled list of `key: value` details.
#[derive(Debug, Clone)]
pub struct CrashReportCategory {
    title: String,
    entries: Vec<(String, String)>,
}

impl CrashReportCategory {
    pub fn new(title: impl Into<String>) -> Self {
        Self { title: title.into(), entries: Vec::new() }
    }

    pub fn set_detail(&mut self, key: impl Into<String>, value: impl Display) -> &mut Self {
        self.entries.push((key.into(), value.to_string()));
        self
    }

    /// `setDetail(key, CrashReportDetail)`: a panicking supplier is recorded as `~~ERROR~~ ...` like
    /// `setDetailError`.
    pub fn set_detail_with(&mut self, key: impl Into<String>, supplier: impl FnOnce() -> String) -> &mut Self {
        let key = key.into();
        match panic::catch_unwind(AssertUnwindSafe(supplier)) {
            Ok(value) => self.entries.push((key, value)),
            Err(payload) => self.entries.push((key, format!("~~ERROR~~ {}", panic_message(payload.as_ref())))),
        }
        self
    }

    fn write_details(&self, out: &mut String) {
        out.push_str("-- ");
        out.push_str(&self.title);
        out.push_str(" --\nDetails:");
        for (key, value) in &self.entries {
            out.push_str("\n\t");
            out.push_str(key);
            out.push_str(": ");
            out.push_str(value);
        }
    }
}

/// `SystemReport`: the `-- System Details --` block.
#[derive(Debug, Clone)]
pub struct SystemReport {
    entries: Vec<(String, String)>,
}

impl Default for SystemReport {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemReport {
    pub fn new() -> Self {
        let mut report = Self { entries: Vec::new() };
        report.set_detail("Minecraft Version", WORLD_VERSION_NAME);
        report.set_detail("Minecraft Version ID", WORLD_VERSION_NAME);
        report.set_detail("Operating System", format!("{} ({}) version {}", os_name(), std::env::consts::ARCH, os_version()));
        report.set_detail("CPUs", std::thread::available_parallelism().map_or(1, |n| n.get()).to_string());
        report
    }

    /// `setDetail`: replaces the value of an existing key (vanilla's `LinkedHashMap`-like `put`), else appends.
    pub fn set_detail(&mut self, key: impl Into<String>, value: impl Display) {
        let key = key.into();
        let value = value.to_string();
        match self.entries.iter_mut().find(|(k, _)| *k == key) {
            Some(entry) => entry.1 = value,
            None => self.entries.push((key, value)),
        }
    }

    pub fn set_detail_with(&mut self, key: impl Into<String>, supplier: impl FnOnce() -> String) {
        let key = key.into();
        let value = match panic::catch_unwind(AssertUnwindSafe(supplier)) {
            Ok(value) => value,
            Err(payload) => format!("~~ERROR~~ {}", panic_message(payload.as_ref())),
        };
        self.set_detail(key, value);
    }

    pub fn append_to_crash_report_string(&self, out: &mut String) {
        out.push_str("-- System Details --\nDetails:");
        for (key, value) in &self.entries {
            out.push_str("\n\t");
            out.push_str(key);
            out.push_str(": ");
            out.push_str(value);
        }
    }
}

fn os_name() -> String {
    read_os_release("NAME").unwrap_or_else(|| std::env::consts::OS.to_owned())
}

fn os_version() -> String {
    read_os_release("VERSION_ID").unwrap_or_else(|| "unknown".to_owned())
}

fn read_os_release(key: &str) -> Option<String> {
    let text = std::fs::read_to_string("/etc/os-release").ok()?;
    text.lines().find_map(|line| line.strip_prefix(key)?.strip_prefix('=').map(|v| v.trim_matches('"').to_owned()))
}

/// `CrashReport`.
#[derive(Debug, Clone)]
pub struct CrashReport {
    title: String,
    /// What Java prints for `exception.printStackTrace()`: `Class: message` and the frames.
    exception: String,
    details: Vec<CrashReportCategory>,
    system_report: SystemReport,
    save_file: Option<PathBuf>,
}

impl CrashReport {
    pub fn new(title: impl Into<String>, exception: impl Into<String>) -> Self {
        Self { title: title.into(), exception: exception.into(), details: Vec::new(), system_report: SystemReport::new(), save_file: None }
    }

    /// `CrashReport.forThrowable`: a [`ReportedException`] payload yields its own report; anything else gets a new
    /// report titled `title` whose exception text is the panic message and captured backtrace.
    pub fn for_throwable(payload: Box<dyn Any + Send>, title: &str) -> Self {
        match payload.downcast::<ReportedException>() {
            Ok(reported) => reported.0,
            Err(payload) => Self::new(title, exception_text(payload.as_ref())),
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn exception(&self) -> &str {
        &self.exception
    }

    pub fn system_report(&mut self) -> &mut SystemReport {
        &mut self.system_report
    }

    pub fn add_category(&mut self, name: impl Into<String>) -> &mut CrashReportCategory {
        self.details.push(CrashReportCategory::new(name));
        self.details.last_mut().expect("just pushed")
    }

    pub fn save_file(&self) -> Option<&Path> {
        self.save_file.as_deref()
    }

    pub fn get_details(&self) -> String {
        let mut out = String::new();
        for category in &self.details {
            category.write_details(&mut out);
            out.push_str("\n\n");
        }
        self.system_report.append_to_crash_report_string(&mut out);
        out
    }

    pub fn get_friendly_report(&self) -> String {
        let mut out = String::new();
        out.push_str("---- ");
        out.push_str(CRASH_HEADER);
        out.push_str(" ----\n// ");
        out.push_str(error_comment());
        out.push_str("\n\n");
        out.push_str("Time: ");
        out.push_str(&chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
        out.push_str("\nDescription: ");
        out.push_str(&self.title);
        out.push_str("\n\n");
        out.push_str(&self.exception);
        out.push_str("\n\nA detailed walkthrough of the error, its code path and all known details is as follows:\n");
        out.push_str(&"-".repeat(87));
        out.push_str("\n\n");
        out.push_str(&self.get_details());
        out
    }

    /// `saveToFile`: `false` if already saved or on I/O failure (logged like vanilla).
    pub fn save_to_file(&mut self, file: &Path) -> bool {
        if self.save_file.is_some() {
            return false;
        }
        let result = (|| -> std::io::Result<()> {
            if let Some(parent) = file.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)?;
            }
            let mut writer = std::io::BufWriter::new(std::fs::File::create(file)?);
            writer.write_all(self.get_friendly_report().as_bytes())?;
            writer.flush()
        })();
        match result {
            Ok(()) => {
                self.save_file = Some(file.to_path_buf());
                true
            }
            Err(e) => {
                log::error!("Could not save crash report to {}\n{e}", file.display());
                false
            }
        }
    }
}

/// `ReportErrorComment`: `nuggets.get((int)(Util.getNanos() % size))`.
fn error_comment() -> &'static str {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    CRASH_NUGGETS[(nanos % CRASH_NUGGETS.len() as u128) as usize]
}

/// `ReportedException`: thrown (panic payload) to abort with a prepared [`CrashReport`]. Use
/// [`ReportedException::throw`].
#[derive(Debug)]
pub struct ReportedException(pub CrashReport);

impl ReportedException {
    pub fn throw(report: CrashReport) -> ! {
        panic::resume_unwind(Box::new(ReportedException(report)))
    }
}

/// `Util.filenameFormattedDateTime`: `yyyy-MM-dd_HH.mm.ss`.
pub fn filename_formatted_date_time() -> String {
    chrono::Local::now().format("%Y-%m-%d_%H.%M.%S").to_string()
}

/// Message of a panic payload (`&str` / `String` / [`ReportedException`] title).
pub fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else if let Some(r) = payload.downcast_ref::<ReportedException>() {
        r.0.title.clone()
    } else {
        "unknown error".to_owned()
    }
}

thread_local! {
    /// Backtrace captured by the panic hook for the last panic on this thread.
    static LAST_PANIC_TRACE: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Message + location + backtrace of `payload` (the trace is the one the hook recorded for the last panic on this
/// thread, if any).
pub fn exception_text(payload: &(dyn Any + Send)) -> String {
    let mut text = panic_message(payload);
    if let Some(trace) = LAST_PANIC_TRACE.with(|t| t.borrow_mut().take()) {
        text.push('\n');
        text.push_str(&trace);
    }
    text
}

/// Record location + backtrace of every panic (so a crash report built after `catch_unwind` has them), then
/// defer to the previously installed hook except for [`ReportedException`]s (which are control flow, not bugs).
pub fn install_panic_hook() {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if info.payload().is::<ReportedException>() {
            return;
        }
        let location = info.location().map_or_else(String::new, |l| format!("\tat {}:{}:{}\n", l.file(), l.line(), l.column()));
        let trace = format!("{location}{}", std::backtrace::Backtrace::force_capture());
        LAST_PANIC_TRACE.with(|t| *t.borrow_mut() = Some(trace));
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_layout_matches_vanilla() {
        let mut report = CrashReport::new("Exception in server tick loop", "IllegalStateException: boom");
        report.add_category("Performance stats").set_detail("Random tick rate", 3);
        report.system_report().set_detail("Server Running", "true");
        let text = report.get_friendly_report();
        assert!(text.starts_with("---- Minecraft Crash Report ----\n// "));
        assert!(text.contains("\n\nTime: "));
        assert!(text.contains("\nDescription: Exception in server tick loop\n\nIllegalStateException: boom\n\nA detailed walkthrough of the error, its code path and all known details is as follows:\n"));
        assert!(text.contains(&format!("{}\n\n-- Performance stats --\nDetails:\n\tRandom tick rate: 3\n\n-- System Details --\nDetails:\n\tMinecraft Version: 26.3", "-".repeat(87))));
        assert!(text.ends_with("\n\tServer Running: true"));
    }

    #[test]
    fn reported_exception_roundtrip() {
        let caught = panic::catch_unwind(|| ReportedException::throw(CrashReport::new("t", "e"))).unwrap_err();
        let report = CrashReport::for_throwable(caught, "other");
        assert_eq!(report.title(), "t");
    }

    #[test]
    fn save_once() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("crash-reports").join("crash.txt");
        let mut report = CrashReport::new("t", "e");
        assert!(report.save_to_file(&file));
        assert!(!report.save_to_file(&file));
        assert!(std::fs::read_to_string(&file).unwrap().contains("Description: t"));
    }
}
