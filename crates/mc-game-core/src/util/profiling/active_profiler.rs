//! `ActiveProfiler`.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use super::MetricCategory;
use super::results::{FilledProfileResults, PathEntrySnapshot, ProfileResults};

/// `ActiveProfiler.WARNING_TIME_NANOS`.
const WARNING_TIME_NANOS: i64 = 100_000_000;
/// `ProfileResults.PATH_SEPARATOR`.
pub(super) const PATH_SEPARATOR: char = '\u{1e}';

#[derive(Clone, Debug)]
struct PathEntry {
    max_duration: i64,
    min_duration: i64,
    accumulated_duration: i64,
    count: i64,
    counters: BTreeMap<String, i64>,
}

impl Default for PathEntry {
    fn default() -> Self {
        Self {
            max_duration: i64::MIN,
            min_duration: i64::MAX,
            accumulated_duration: 0,
            count: 0,
            counters: BTreeMap::new(),
        }
    }
}

pub struct ActiveProfiler {
    paths: Vec<String>,
    start_times: Vec<i64>,
    entries: HashMap<String, PathEntry>,
    get_tick_time: Arc<dyn Fn() -> i32 + Send + Sync>,
    get_real_time: Arc<dyn Fn() -> i64 + Send + Sync>,
    start_time_nano: i64,
    start_time_ticks: i32,
    path: String,
    started: bool,
    suppress_warnings: bool,
    charted_paths: Vec<(String, MetricCategory)>,
}

impl ActiveProfiler {
    pub fn new(
        get_real_time: Arc<dyn Fn() -> i64 + Send + Sync>,
        get_tick_time: Arc<dyn Fn() -> i32 + Send + Sync>,
        suppress_warnings: bool,
    ) -> Self {
        Self {
            paths: Vec::new(),
            start_times: Vec::new(),
            entries: HashMap::new(),
            start_time_nano: get_real_time(),
            start_time_ticks: get_tick_time(),
            get_real_time,
            get_tick_time,
            path: String::new(),
            started: false,
            suppress_warnings,
            charted_paths: Vec::new(),
        }
    }

    pub fn start_tick(&mut self) {
        if self.started {
            log::error!("Profiler tick already started - missing endTick()?");
        } else {
            self.started = true;
            self.path.clear();
            self.paths.clear();
            self.push("root");
        }
    }

    pub fn end_tick(&mut self) {
        if !self.started {
            log::error!("Profiler tick already ended - missing startTick()?");
        } else {
            self.pop();
            self.started = false;
            if !self.path.is_empty() {
                log::error!(
                    "Profiler tick ended before path was fully popped (remainder: '{}'). Mismatched push/pop?",
                    demangle_path(&self.path)
                );
            }
        }
    }

    pub fn push(&mut self, name: &str) {
        if !self.started {
            log::error!(
                "Cannot push '{name}' to profiler if profiler tick hasn't started - missing startTick()?"
            );
        } else {
            if !self.path.is_empty() {
                self.path.push(PATH_SEPARATOR);
            }
            self.path.push_str(name);
            self.paths.push(self.path.clone());
            self.start_times.push(crate::util::util::get_nanos());
        }
    }

    pub fn mark_for_charting(&mut self, category: MetricCategory) {
        let entry = (self.path.clone(), category);
        if !self.charted_paths.contains(&entry) {
            self.charted_paths.push(entry);
        }
    }

    pub fn pop(&mut self) {
        if !self.started {
            log::error!(
                "Cannot pop from profiler if profiler tick hasn't started - missing startTick()?"
            );
        } else if self.start_times.is_empty() {
            log::error!("Tried to pop one too many times! Mismatched push() and pop()?");
        } else {
            let end_time = crate::util::util::get_nanos();
            let start_time = self.start_times.pop().expect("checked");
            self.paths.pop();
            let time = end_time - start_time;
            let entry = self.entries.entry(self.path.clone()).or_default();
            entry.accumulated_duration += time;
            entry.count += 1;
            entry.max_duration = entry.max_duration.max(time);
            entry.min_duration = entry.min_duration.min(time);
            if time > WARNING_TIME_NANOS && !self.suppress_warnings {
                log::warn!(
                    "Something's taking too long! '{}' took approximately {} ms",
                    demangle_path(&self.path),
                    time as f64 / 1_000_000.0
                );
            }
            self.path = self.paths.last().cloned().unwrap_or_default();
        }
    }

    pub fn pop_push(&mut self, name: &str) {
        self.pop();
        self.push(name);
    }

    pub fn increment_counter(&mut self, name: &str, amount: i32) {
        *self
            .entries
            .entry(self.path.clone())
            .or_default()
            .counters
            .entry(name.to_owned())
            .or_insert(0) += i64::from(amount);
    }

    /// `getResults()`.
    pub fn get_results(&self) -> ProfileResults {
        ProfileResults::Filled(FilledProfileResults::new(
            self.entries
                .iter()
                .map(|(k, e)| {
                    (
                        k.clone(),
                        PathEntrySnapshot {
                            duration: e.accumulated_duration,
                            count: e.count,
                            counters: e.counters.clone(),
                        },
                    )
                })
                .collect(),
            self.start_time_nano,
            self.start_time_ticks,
            (self.get_real_time)(),
            (self.get_tick_time)(),
        ))
    }

    /// `getEntry(path)`: `maxDuration` (the metric samplers of charted paths read it).
    pub fn get_max_duration(&self, path: &str) -> Option<i64> {
        self.entries.get(path).map(|e| e.max_duration)
    }

    pub fn get_charted_paths(&self) -> &[(String, MetricCategory)] {
        &self.charted_paths
    }
}

/// `ProfileResults.demanglePath`.
pub fn demangle_path(path: &str) -> String {
    path.replace(PATH_SEPARATOR, ".")
}
