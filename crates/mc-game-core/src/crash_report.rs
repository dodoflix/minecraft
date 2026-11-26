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

/// `ReportType`: the header and the comments one of which heads a report file.
#[derive(Debug, Clone, Copy)]
pub struct ReportType {
    header: &'static str,
    nuggets: &'static [&'static str],
}

impl ReportType {
    pub const CRASH: ReportType = ReportType {
        header: "Minecraft Crash Report",
        nuggets: &CRASH_NUGGETS,
    };
    pub const PROFILE: ReportType = ReportType {
        header: "Minecraft Profiler Results",
        nuggets: &PROFILE_NUGGETS,
    };
    pub const TEST: ReportType = ReportType {
        header: "Minecraft Test Report",
        nuggets: &TEST_NUGGETS,
    };
    pub const NETWORK_PROTOCOL_ERROR: ReportType = ReportType {
        header: "Minecraft Network Protocol Error Report",
        nuggets: &NETWORK_PROTOCOL_ERROR_NUGGETS,
    };
    pub const CHUNK_IO_ERROR: ReportType = ReportType {
        header: "Minecraft Chunk IO Error Report",
        nuggets: &CHUNK_IO_ERROR_NUGGETS,
    };

    /// `getErrorComment`: `nuggets.get((int)(Util.getNanos() % size))`.
    fn error_comment(&self) -> &'static str {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        self.nuggets[(nanos % self.nuggets.len() as u128) as usize]
    }

    /// `appendHeader`: the title line, the witty comment, one `// ` line per extra comment and a blank line.
    pub fn append_header(&self, out: &mut String, extra_comments: &[&str]) {
        out.push_str("---- ");
        out.push_str(self.header);
        out.push_str(" ----\n// ");
        out.push_str(self.error_comment());
        out.push('\n');
        for comment in extra_comments {
            out.push_str("// ");
            out.push_str(comment);
            out.push('\n');
        }
        out.push('\n');
    }
}

/// `ReportType.TEST.nuggets`.
const TEST_NUGGETS: [&str; 8] = [
    "Don't mind me",
    "One day I will be a real crash!",
    "Booo! Haha, did I scare you?",
    "Help, I'm trapped in a report factory!",
    "Have I answered your question?",
    "No hugs here, sorry",
    "I Can't Believe It's Not A Crash Report!",
    "Where's the kaboom!?",
];

/// `ReportType.NETWORK_PROTOCOL_ERROR.nuggets`.
const NETWORK_PROTOCOL_ERROR_NUGGETS: [&str; 19] = [
    "0xBADF00D",
    "+'${`%&NO CARRIER",
    "Please insert The Internet CD #4",
    "Sabotage!",
    "Are you sure you are not moving wrongly?",
    "This time is not my fault, I promise!",
    "All lines are down!",
    "Maybe a shark bit some cable",
    "404",
    "I'm sorry, I don't speak that language",
    "What we've got here is failure to communicate",
    "It's the tubes, they're clogged!",
    "Abort, Retry, Ignore?",
    "Could be worse, I guess",
    "Wait, was the last bit one or zero?",
    "Too many suspicious packets",
    "Don't worry, I'll be fine",
    "Maybe this time it will work!",
    "I heard pigeons are more reliable",
];

const CHUNK_IO_ERROR_NUGGETS: [&str; 14] = [
    "I have failed you!",
    "Let's not do it again...",
    "Worst magic trick ever!",
    "Remember to backup your worlds regularly",
    "Pirates stole your chunk!",
    "Ker-chunk!",
    "Ideally, this shouldn't be here",
    "Let's hope it wasn't anything important",
    "Computers were a mistake",
    "Welp",
    "Not my proudest moment",
    "Who needs blocks in a block game, right?",
    "This chunk is no more...it has ceased to be...this is an EX-chunk",
    "loss.mca",
];

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
    /// `stackTrace`: the frames from where the category was added, one `symbol(file:line)` each.
    stack_trace: Vec<String>,
}

impl CrashReportCategory {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            entries: Vec::new(),
            stack_trace: Vec::new(),
        }
    }

    pub fn set_detail(&mut self, key: impl Into<String>, value: impl Display) -> &mut Self {
        self.entries.push((key.into(), value.to_string()));
        self
    }

    /// `setDetail(key, CrashReportDetail)`: a panicking supplier is recorded as `~~ERROR~~ ...` like
    /// `setDetailError`.
    pub fn set_detail_with(
        &mut self,
        key: impl Into<String>,
        supplier: impl FnOnce() -> String,
    ) -> &mut Self {
        let key = key.into();
        match panic::catch_unwind(AssertUnwindSafe(supplier)) {
            Ok(value) => self.entries.push((key, value)),
            Err(payload) => self.entries.push((
                key,
                format!("~~ERROR~~ {}", panic_message(payload.as_ref())),
            )),
        }
        self
    }

    /// `formatLocation(levelHeightAccessor, x, y, z)`: the block, its section and its region.
    pub fn format_location(
        level_height: mc_world::chunk::LevelHeight,
        x: i32,
        y: i32,
        z: i32,
    ) -> String {
        let (section_x, section_y, section_z) = (x >> 4, y >> 4, z >> 4);
        let min_block_x = section_x.wrapping_shl(4);
        let min_block_z = section_z.wrapping_shl(4);
        let max_block_x = section_x.wrapping_add(1).wrapping_shl(4).wrapping_sub(1);
        let max_block_z = section_z.wrapping_add(1).wrapping_shl(4).wrapping_sub(1);
        let (min_y, max_y) = (level_height.min_y, level_height.max_y());
        let (region_x, region_z) = (x >> 9, z >> 9);
        let min_chunk_x = region_x.wrapping_shl(5);
        let min_chunk_z = region_z.wrapping_shl(5);
        let max_chunk_x = region_x.wrapping_add(1).wrapping_shl(5).wrapping_sub(1);
        let max_chunk_z = region_z.wrapping_add(1).wrapping_shl(5).wrapping_sub(1);
        let region_min_x = region_x.wrapping_shl(9);
        let region_min_z = region_z.wrapping_shl(9);
        let region_max_x = region_x.wrapping_add(1).wrapping_shl(9).wrapping_sub(1);
        let region_max_z = region_z.wrapping_add(1).wrapping_shl(9).wrapping_sub(1);
        format!(
            "World: ({x},{y},{z}), Section: (at {},{},{} in {section_x},{section_y},{section_z}; chunk contains blocks \
             {min_block_x},{min_y},{min_block_z} to {max_block_x},{max_y},{max_block_z}), Region: ({region_x},{region_z}; \
             contains chunks {min_chunk_x},{min_chunk_z} to {max_chunk_x},{max_chunk_z}, blocks \
             {region_min_x},{min_y},{region_min_z} to {region_max_x},{max_y},{region_max_z})",
            x & 15,
            y & 15,
            z & 15
        )
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
        if !self.stack_trace.is_empty() {
            out.push_str("\nStacktrace:");
            for frame in &self.stack_trace {
                out.push_str("\n\tat ");
                out.push_str(frame);
            }
        }
    }
}

/// `fillInStackTrace`: the frames of the calling thread below `CrashReport::add_category`, as `symbol(file:line)`.
fn capture_stack_trace() -> Vec<String> {
    let text = std::backtrace::Backtrace::force_capture().to_string();
    let mut frames: Vec<(String, Option<String>)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(location) = line.strip_prefix("at ") {
            let mut parts = location.rsplitn(3, ':');
            if let (Some(last), Some(_column), Some(line_no), Some(path)) =
                (frames.last_mut(), parts.next(), parts.next(), parts.next())
            {
                let name = path.rsplit('/').next().unwrap_or(path);
                last.1 = Some(format!("{name}:{line_no}"));
            }
        } else if let Some((index, symbol)) = line.split_once(": ")
            && index.bytes().all(|b| b.is_ascii_digit())
        {
            frames.push((symbol.to_owned(), None));
        }
    }
    let below = frames
        .iter()
        .rposition(|(symbol, _)| symbol.contains("CrashReport::add_category"))
        .map_or(0, |i| i + 1);
    frames
        .into_iter()
        .skip(below)
        .map(|(symbol, location)| {
            format!(
                "{symbol}({})",
                location.as_deref().unwrap_or("Unknown Source")
            )
        })
        .collect()
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
        let mut report = Self {
            entries: Vec::new(),
        };
        report.set_detail("Minecraft Version", WORLD_VERSION_NAME);
        report.set_detail("Minecraft Version ID", WORLD_VERSION_NAME);
        report.set_detail(
            "Operating System",
            format!(
                "{} ({}) version {}",
                os_name(),
                std::env::consts::ARCH,
                os_version()
            ),
        );
        report.set_detail(
            "CPUs",
            std::thread::available_parallelism()
                .map_or(1, |n| n.get())
                .to_string(),
        );
        report
    }

    /// `setDetail`: appends a row; a key that is set twice appears twice.
    pub fn set_detail(&mut self, key: impl Into<String>, value: impl Display) {
        self.entries.push((key.into(), value.to_string()));
    }

    pub fn set_detail_with(&mut self, key: impl Into<String>, supplier: impl FnOnce() -> String) {
        let key = key.into();
        let value = match panic::catch_unwind(AssertUnwindSafe(supplier)) {
            Ok(value) => value,
            Err(payload) => format!("~~ERROR~~ {}", panic_message(payload.as_ref())),
        };
        self.set_detail(key, value);
    }

    /// `toLineSeparatedString()`: `key: value` lines.
    pub fn to_line_separated_string(&self) -> String {
        self.entries
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect::<Vec<_>>()
            .join("\n")
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
    text.lines().find_map(|line| {
        line.strip_prefix(key)?
            .strip_prefix('=')
            .map(|v| v.trim_matches('"').to_owned())
    })
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
        Self {
            title: title.into(),
            exception: exception.into(),
            details: Vec::new(),
            system_report: SystemReport::new(),
            save_file: None,
        }
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
        let mut category = CrashReportCategory::new(name);
        category.stack_trace = capture_stack_trace();
        self.details.push(category);
        self.details.last_mut().expect("just pushed")
    }

    pub fn save_file(&self) -> Option<&Path> {
        self.save_file.as_deref()
    }

    pub fn get_details(&self) -> String {
        let mut out = String::new();
        // the uncategorized trace is the first frame of the first category's own
        if let Some(frame) = self.details.first().and_then(|c| c.stack_trace.first()) {
            out.push_str("-- Head --\nThread: ");
            out.push_str(std::thread::current().name().unwrap_or("<unnamed>"));
            out.push_str("\nStacktrace:\n\tat ");
            out.push_str(frame);
            out.push_str("\n\n");
        }
        for category in &self.details {
            category.write_details(&mut out);
            out.push_str("\n\n");
        }
        self.system_report.append_to_crash_report_string(&mut out);
        out
    }

    pub fn get_friendly_report(&self, report_type: ReportType) -> String {
        self.get_friendly_report_with(report_type, &[])
    }

    pub fn get_friendly_report_with(
        &self,
        report_type: ReportType,
        extra_comments: &[&str],
    ) -> String {
        let mut out = String::new();
        report_type.append_header(&mut out, extra_comments);
        out.push_str("Time: ");
        out.push_str(&chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
        out.push_str("\nDescription: ");
        out.push_str(&self.title);
        out.push_str("\n\n");
        // `printStackTrace` ends its last line with a newline
        out.push_str(self.exception.trim_end_matches('\n'));
        out.push_str("\n\n\nA detailed walkthrough of the error, its code path and all known details is as follows:\n");
        out.push_str(&"-".repeat(87));
        out.push_str("\n\n");
        out.push_str(&self.get_details());
        out
    }

    /// `saveToFile`: `false` if already saved or on I/O failure (logged like vanilla).
    pub fn save_to_file(&mut self, file: &Path, report_type: ReportType) -> bool {
        if self.save_file.is_some() {
            return false;
        }
        let result = (|| -> std::io::Result<()> {
            if let Some(parent) = file.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)?;
            }
            let mut writer = std::io::BufWriter::new(std::fs::File::create(file)?);
            writer.write_all(self.get_friendly_report(report_type).as_bytes())?;
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
        let location = info.location().map_or_else(String::new, |l| {
            format!("\tat {}:{}:{}\n", l.file(), l.line(), l.column())
        });
        let trace = format!("{location}{}", std::backtrace::Backtrace::force_capture());
        LAST_PANIC_TRACE.with(|t| *t.borrow_mut() = Some(trace));
        previous(info);
    }));
}

/// `ReportType.PROFILE.nuggets`.
const PROFILE_NUGGETS: [&str; 15] = [
    "I'd Rather Be Surfing",
    "Shiny numbers!",
    "Am I not running fast enough? :(",
    "I'm working as hard as I can!",
    "Will I ever be good enough for you? :(",
    "Speedy. Zoooooom!",
    "Hello world",
    "40% better than a crash report.",
    "Now with extra numbers",
    "Now with less numbers",
    "Now with the same numbers",
    "You should add flames to things, it makes them go faster!",
    "Do you feel the need for... optimization?",
    "*cracks redstone whip*",
    "Maybe if you treated it better then it'll have more motivation to work faster! Poor server.",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reported_exception_roundtrip() {
        let caught = panic::catch_unwind(|| ReportedException::throw(CrashReport::new("t", "e")))
            .unwrap_err();
        let report = CrashReport::for_throwable(caught, "other");
        assert_eq!(report.title(), "t");
    }

    #[test]
    fn save_once() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("crash-reports").join("crash.txt");
        let mut report = CrashReport::new("t", "e");
        assert!(report.save_to_file(&file, ReportType::CRASH));
        assert!(!report.save_to_file(&file, ReportType::CRASH));
        assert!(
            std::fs::read_to_string(&file)
                .unwrap()
                .contains("Description: t")
        );
    }
}
