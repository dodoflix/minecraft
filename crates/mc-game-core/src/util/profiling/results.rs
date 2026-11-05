//! `ProfileResults`, `FilledProfileResults`, `ResultField`.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::Path;

use super::active_profiler::PATH_SEPARATOR;
use crate::util::java_format::format_fixed;

/// What `ProfilerPathEntry` exposes of a path.
#[derive(Clone, Debug, Default)]
pub struct PathEntrySnapshot {
    pub duration: i64,
    pub count: i64,
    pub counters: BTreeMap<String, i64>,
}

/// `ResultField`.
#[derive(Clone, Debug)]
pub struct ResultField {
    pub percentage: f64,
    pub global_percentage: f64,
    pub count: i64,
    pub name: String,
}

impl ResultField {
    /// `compareTo`: higher percentage first, then names in reverse order.
    fn compare(&self, other: &ResultField) -> Ordering {
        if other.percentage < self.percentage {
            Ordering::Less
        } else if other.percentage > self.percentage {
            Ordering::Greater
        } else {
            // `resultField.name.compareTo(this.name)` (UTF-16 code unit order)
            other.name.encode_utf16().cmp(self.name.encode_utf16())
        }
    }
}

/// `ProfileResults` (`EmptyProfileResults.EMPTY` or `FilledProfileResults`).
#[derive(Clone, Debug)]
pub enum ProfileResults {
    Empty,
    Filled(FilledProfileResults),
}

impl ProfileResults {
    pub fn get_nano_duration(&self) -> i64 {
        match self {
            Self::Empty => 0,
            Self::Filled(r) => r.end_time_nano - r.start_time_nano,
        }
    }

    pub fn get_tick_duration(&self) -> i32 {
        match self {
            Self::Empty => 0,
            Self::Filled(r) => r.tick_duration,
        }
    }

    pub fn save_results(&self, file: &Path) -> bool {
        match self {
            Self::Empty => false,
            Self::Filled(r) => r.save_results(file),
        }
    }
}

#[derive(Clone, Debug)]
pub struct FilledProfileResults {
    entries: HashMap<String, PathEntrySnapshot>,
    start_time_nano: i64,
    end_time_nano: i64,
    tick_duration: i32,
}

#[derive(Default)]
struct CounterCollector {
    self_value: i64,
    total_value: i64,
    children: HashMap<String, CounterCollector>,
}

impl CounterCollector {
    fn add_value(&mut self, path: &mut std::slice::Iter<'_, &str>, value: i64) {
        self.total_value += value;
        match path.next() {
            None => self.self_value += value,
            Some(segment) => self.children.entry((*segment).to_owned()).or_default().add_value(path, value),
        }
    }
}

/// `String.format("%.2f", float)`-style helper over `f32` input widened like Java's boxing (`Float` -> `double`).
fn fixed(value: f64, digits: usize) -> String {
    format_fixed(value, digits)
}

impl FilledProfileResults {
    pub fn new(entries: HashMap<String, PathEntrySnapshot>, start_time_nano: i64, start_time_ticks: i32, end_time_nano: i64, end_time_ticks: i32) -> Self {
        Self { entries, start_time_nano, end_time_nano, tick_duration: end_time_ticks.wrapping_sub(start_time_ticks) }
    }

    fn entry(&self, path: &str) -> PathEntrySnapshot {
        self.entries.get(path).cloned().unwrap_or_default()
    }

    fn is_direct_child(path: &str, test: &str) -> bool {
        test.len() > path.len() && test.starts_with(path) && !test[path.len() + 1..].contains(PATH_SEPARATOR)
    }

    /// `getTimes(path)`.
    pub fn get_times(&self, path: &str) -> Vec<ResultField> {
        let raw_path = path;
        let mut global_time = self.entry("root").duration;
        let current = self.entry(path);
        let self_time = current.duration;
        let self_count = current.count;
        let prefix = if path.is_empty() { String::new() } else { format!("{path}{PATH_SEPARATOR}") };
        let mut total_time: i64 = 0;
        for key in self.entries.keys() {
            if Self::is_direct_child(&prefix, key) {
                total_time += self.entry(key).duration;
            }
        }
        let old_time = total_time as f32;
        if total_time < self_time {
            total_time = self_time;
        }
        if global_time < total_time {
            global_time = total_time;
        }
        let mut result = Vec::new();
        for key in self.entries.keys() {
            if Self::is_direct_child(&prefix, key) {
                let entry = self.entry(key);
                let time = entry.duration;
                result.push(ResultField {
                    name: key[prefix.len()..].to_owned(),
                    percentage: time as f64 * 100.0 / total_time as f64,
                    global_percentage: time as f64 * 100.0 / global_time as f64,
                    count: entry.count,
                });
            }
        }
        if total_time as f32 > old_time {
            let unspecified = (total_time as f32 - old_time) as f64;
            result.push(ResultField {
                name: "unspecified".to_owned(),
                percentage: unspecified * 100.0 / total_time as f64,
                global_percentage: unspecified * 100.0 / global_time as f64,
                count: self_count,
            });
        }
        result.sort_by(|a, b| a.compare(b));
        result.insert(
            0,
            ResultField { name: raw_path.to_owned(), percentage: 100.0, global_percentage: total_time as f64 * 100.0 / global_time as f64, count: self_count },
        );
        result
    }

    fn counter_values(&self) -> BTreeMap<String, CounterCollector> {
        let mut result: BTreeMap<String, CounterCollector> = BTreeMap::new();
        for (path, entry) in &self.entries {
            if !entry.counters.is_empty() {
                let segments: Vec<&str> = path.split(PATH_SEPARATOR).collect();
                for (counter, value) in &entry.counters {
                    result.entry(counter.clone()).or_default().add_value(&mut segments.iter(), *value);
                }
            }
        }
        result
    }

    /// `saveResults(file)`.
    pub fn save_results(&self, file: &Path) -> bool {
        let write = || -> std::io::Result<()> {
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(file, self.get_profiler_results_report(self.end_time_nano - self.start_time_nano, self.tick_duration))
        };
        match write() {
            Ok(()) => true,
            Err(e) => {
                log::error!("Could not save profiler results to {}\n{e}", file.display());
                false
            }
        }
    }

    fn get_profiler_results_report(&self, timespan: i64, tickspan: i32) -> String {
        let mut builder = String::new();
        crate::crash_report::append_profile_report_header(&mut builder);
        let _ = writeln!(builder, "Version: {}", crate::server::packs::GAME_VERSION);
        let _ = writeln!(builder, "Time span: {} ms", timespan / 1_000_000);
        let _ = writeln!(builder, "Tick span: {tickspan} ticks");
        let _ = write!(
            builder,
            "// This is approximately {} ticks per second. It should be 20 ticks per second\n\n",
            fixed(f64::from(tickspan as f32 / (timespan as f32 / 1.0E9_f32)), 2)
        );
        builder.push_str("--- BEGIN PROFILE DUMP ---\n\n");
        self.append_profiler_results(0, "root", &mut builder);
        builder.push_str("--- END PROFILE DUMP ---\n\n");
        let counters = self.counter_values();
        if !counters.is_empty() {
            builder.push_str("--- BEGIN COUNTER DUMP ---\n\n");
            Self::append_counters(&counters, &mut builder, tickspan);
            builder.push_str("--- END COUNTER DUMP ---\n\n");
        }
        builder
    }

    fn indent_line(builder: &mut String, depth: usize) {
        let _ = write!(builder, "[{depth:02}] ");
        for _ in 0..depth {
            builder.push_str("|   ");
        }
    }

    fn append_profiler_results(&self, depth: usize, path: &str, builder: &mut String) {
        let results = self.get_times(path);
        if let Some(entry) = self.entries.get(path) {
            for (id, value) in &entry.counters {
                Self::indent_line(builder, depth);
                // `value / this.tickDuration` is a `long / int` division (an ArithmeticException for 0 ticks)
                let _ = writeln!(builder, "#{id} {value}/{}", value.checked_div(i64::from(self.tick_duration)).unwrap_or(0));
            }
        }
        if results.len() >= 3 {
            for result in &results[1..] {
                Self::indent_line(builder, depth);
                let _ = writeln!(
                    builder,
                    "{}({}/{}) - {}%/{}%",
                    result.name,
                    result.count,
                    fixed(f64::from(result.count as f32 / self.tick_duration as f32), 0),
                    fixed(result.percentage, 2),
                    fixed(result.global_percentage, 2)
                );
                if result.name != "unspecified" {
                    self.append_profiler_results(depth + 1, &format!("{path}{PATH_SEPARATOR}{}", result.name), builder);
                }
            }
        }
    }

    fn append_counter_results(depth: usize, name: &str, result: &CounterCollector, tickspan: i32, builder: &mut String) {
        Self::indent_line(builder, depth);
        let ticks = i64::from(tickspan).max(1);
        let _ = writeln!(builder, "{name} total:{}/{} average: {}/{}", result.self_value, result.total_value, result.self_value / ticks, result.total_value / ticks);
        let mut children: Vec<_> = result.children.iter().collect();
        children.sort_by_key(|(_, c)| std::cmp::Reverse(c.total_value));
        for (child_name, child) in children {
            Self::append_counter_results(depth + 1, child_name, child, tickspan, builder);
        }
    }

    fn append_counters(counters: &BTreeMap<String, CounterCollector>, builder: &mut String, tickspan: i32) {
        for (counter, root) in counters {
            let _ = writeln!(builder, "-- Counter: {counter} --");
            if let Some(root_collector) = root.children.get("root") {
                Self::append_counter_results(0, "root", root_collector, tickspan, builder);
            }
            builder.push_str("\n\n");
        }
    }
}
