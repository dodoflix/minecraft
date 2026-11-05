//! `util.profiling.metrics`: `MetricSampler`, `ServerMetricsSamplersProvider`, `ActiveMetricsRecorder` and
//! `MetricsPersister` (what `/perf start|stop` records and writes).
//!
//! Not ported: `MetricsRegistry` samplers (executors / pathfinding, which have no counterpart here) and the
//! `ProfilerSamplerAdapter` (samplers of paths marked for charting; nothing marks one).

use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicI64, Ordering};

use mc_core::math::java::java_double_to_string;
use parking_lot::Mutex;

use super::{ActiveProfiler, MetricCategory, ProfileResults, ProfilerHandle};
use crate::util::file_util::sanitize_to_path_chars;
use crate::util::util::get_nanos;

/// `ActiveMetricsRecorder.PROFILING_MAX_DURATION_SECONDS`.
const PROFILING_MAX_DURATION_SECONDS: i64 = 10;
/// `MetricsPersister.PROFILING_RESULTS_DIR`.
pub const PROFILING_RESULTS_DIR: &str = "debug/profiling";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SamplingPhase {
    Extract,
    EndTick,
}

/// `MetricSampler`.
pub struct MetricSampler {
    pub name: String,
    pub phase: SamplingPhase,
    pub category: MetricCategory,
    sampler: Box<dyn FnMut() -> f64 + Send>,
    before_tick: Option<Box<dyn FnMut() + Send>>,
    threshold: Option<Box<dyn FnMut(f64) -> bool + Send>>,
    ticks: Vec<i32>,
    values: Vec<f64>,
    current_value: f64,
}

impl MetricSampler {
    /// `MetricSampler.create(name, category, sampler)`.
    pub fn create(name: impl Into<String>, category: MetricCategory, sampler: impl FnMut() -> f64 + Send + 'static) -> Self {
        Self {
            name: name.into(),
            phase: SamplingPhase::EndTick,
            category,
            sampler: Box::new(sampler),
            before_tick: None,
            threshold: None,
            ticks: Vec::new(),
            values: Vec::new(),
            current_value: 0.0,
        }
    }

    fn on_start_tick(&mut self) {
        if let Some(before_tick) = &mut self.before_tick {
            before_tick();
        }
    }

    fn on_end_tick(&mut self, current_tick: i32) {
        self.current_value = (self.sampler)();
        self.values.push(self.current_value);
        self.ticks.push(current_tick);
    }

    fn triggers_threshold(&mut self) -> bool {
        let value = self.current_value;
        self.threshold.as_mut().is_some_and(|test| test(value))
    }

    /// `valueAtTick(tick)` (`Int2DoubleMap.get`: 0.0 when absent).
    fn value_at_tick(&self, tick: i32) -> f64 {
        self.ticks.iter().position(|t| *t == tick).map_or(0.0, |i| self.values[i])
    }
}

/// `ServerMetricsSamplersProvider.tickTimeSampler`: the nanoseconds between the start and the end of the tick.
fn tick_time_sampler() -> MetricSampler {
    let started = Arc::new(AtomicI64::new(-1));
    let sampler_started = started.clone();
    let mut sampler = MetricSampler::create("ticktime", MetricCategory::TickLoop, move || {
        let start = sampler_started.swap(-1, Ordering::SeqCst);
        if start < 0 { 0.0 } else { (get_nanos() - start) as f64 }
    });
    sampler.before_tick = Some(Box::new(move || started.store(get_nanos(), Ordering::SeqCst)));
    // `ValueIncreasedByPercentage(2.0F)`
    let mut previous = f64::from_bits(1); // Double.MIN_VALUE
    sampler.threshold = Some(Box::new(move |value| {
        let result = previous != f64::from_bits(1) && !(value <= previous) && (value - previous) / previous >= f64::from(2.0_f32);
        previous = value;
        result
    }));
    sampler
}

/// `ServerMetricsSamplersProvider.CpuStats` over `/proc/stat`.
struct CpuStats {
    previous: Vec<(u64, u64)>,
    current_load: Vec<f64>,
    last_poll_ms: i64,
}

/// (busy + idle total, idle) ticks per logical processor.
fn read_cpu_ticks() -> Vec<(u64, u64)> {
    let Ok(stat) = std::fs::read_to_string("/proc/stat") else { return Vec::new() };
    stat.lines()
        .filter(|l| l.starts_with("cpu") && l.as_bytes().get(3).is_some_and(u8::is_ascii_digit))
        .map(|l| {
            let fields: Vec<u64> = l.split_whitespace().skip(1).filter_map(|v| v.parse().ok()).collect();
            let total: u64 = fields.iter().take(8).sum();
            let idle = fields.get(3).copied().unwrap_or(0) + fields.get(4).copied().unwrap_or(0);
            (total, idle)
        })
        .collect()
}

impl CpuStats {
    fn new() -> Self {
        let previous = read_cpu_ticks();
        let current_load = vec![0.0; previous.len()];
        Self { previous, current_load, last_poll_ms: 0 }
    }

    fn load_for_cpu(&mut self, i: usize) -> f64 {
        let now = crate::util::util::get_epoch_millis();
        if self.last_poll_ms == 0 || self.last_poll_ms + 501 < now {
            let ticks = read_cpu_ticks();
            self.current_load = ticks
                .iter()
                .zip(&self.previous)
                .map(|((total, idle), (previous_total, previous_idle))| {
                    let total_delta = total.saturating_sub(*previous_total);
                    if total_delta == 0 { 0.0 } else { 1.0 - idle.saturating_sub(*previous_idle) as f64 / total_delta as f64 }
                })
                .collect();
            self.previous = ticks;
            self.last_poll_ms = now;
        }
        self.current_load.get(i).copied().unwrap_or(0.0) * 100.0
    }
}

/// `SystemReport.sizeInMiB(Runtime used memory)`: here the resident set of the process.
fn resident_mib() -> f64 {
    let pages: u64 = std::fs::read_to_string("/proc/self/statm").ok().and_then(|s| s.split_whitespace().nth(1)?.parse().ok()).unwrap_or(0);
    f64::from(((pages * 4096) as i64) as f32 / 1048576.0_f32)
}

/// `new ServerMetricsSamplersProvider(timeSource, isDedicatedServer).samplers(..)`.
fn server_samplers(dedicated: bool) -> Vec<MetricSampler> {
    let mut samplers = vec![tick_time_sampler()];
    if dedicated {
        let cpus = read_cpu_ticks().len();
        let stats = Arc::new(Mutex::new(CpuStats::new()));
        for i in 0..cpus {
            let stats = stats.clone();
            samplers.push(MetricSampler::create(format!("cpu#{i}"), MetricCategory::Cpu, move || stats.lock().load_for_cpu(i)));
        }
        samplers.push(MetricSampler::create("heap MiB", MetricCategory::Jvm, resident_mib));
    }
    samplers
}

/// `RecordedDeviation`.
struct RecordedDeviation {
    timestamp: chrono::DateTime<chrono::Local>,
    tick: i32,
    profiler_result_at_tick: ProfileResults,
}

/// What `scheduleSaveResults` hands to the IO thread: the samplers and deviations of the finished recording.
pub struct SaveJob {
    samplers: Vec<MetricSampler>,
    deviations: Vec<(String, RecordedDeviation)>,
    results: ProfileResults,
}

/// `ActiveMetricsRecorder`.
pub struct ActiveMetricsRecorder {
    samplers: Vec<MetricSampler>,
    deviations: Vec<(String, RecordedDeviation)>,
    task_profiler: ProfilerHandle,
    single_tick_profiler: Option<ProfilerHandle>,
    current_tick: Arc<AtomicI32>,
    deadline_nano: i64,
    kill_switch: bool,
}

impl ActiveMetricsRecorder {
    /// `ActiveMetricsRecorder.createStarted(new ServerMetricsSamplersProvider(..), ..)`.
    pub fn create_started(dedicated: bool) -> Self {
        let current_tick = Arc::new(AtomicI32::new(0));
        let tick = current_tick.clone();
        let task_profiler = Arc::new(Mutex::new(ActiveProfiler::new(Arc::new(get_nanos), Arc::new(move || tick.load(Ordering::SeqCst)), false)));
        let mut recorder = Self {
            samplers: server_samplers(dedicated),
            deviations: Vec::new(),
            task_profiler,
            single_tick_profiler: None,
            current_tick,
            deadline_nano: get_nanos() + PROFILING_MAX_DURATION_SECONDS * 1_000_000_000,
            kill_switch: false,
        };
        recorder.single_tick_profiler = Some(recorder.new_single_tick_profiler());
        recorder
    }

    fn new_single_tick_profiler(&self) -> ProfilerHandle {
        let tick = self.current_tick.clone();
        Arc::new(Mutex::new(ActiveProfiler::new(Arc::new(get_nanos), Arc::new(move || tick.load(Ordering::SeqCst)), true)))
    }

    /// `getProfiler()`: the profilers `Profiler.use` installs for the tick.
    pub fn profilers(&self) -> Vec<ProfilerHandle> {
        let mut profilers = vec![self.task_profiler.clone()];
        profilers.extend(self.single_tick_profiler.clone());
        profilers
    }

    /// `end()`: finish after the current tick.
    pub fn end(&mut self) {
        self.kill_switch = true;
    }

    pub fn start_tick(&mut self) {
        for sampler in &mut self.samplers {
            sampler.on_start_tick();
        }
        self.current_tick.fetch_add(1, Ordering::SeqCst);
    }

    fn sample(&mut self, phase: SamplingPhase) {
        let tick = self.current_tick.load(Ordering::SeqCst);
        if tick == 0 {
            return;
        }
        for sampler in &mut self.samplers {
            if sampler.phase == phase {
                sampler.on_end_tick(tick);
                if sampler.triggers_threshold() {
                    let results = self.single_tick_profiler.as_ref().map_or(ProfileResults::Empty, |p| p.lock().get_results());
                    self.deviations.push((sampler.name.clone(), RecordedDeviation { timestamp: chrono::Local::now(), tick, profiler_result_at_tick: results }));
                }
            }
        }
    }

    /// `endTick()`: `Some((results, job))` when the recording ended (kill switch or 10 s deadline).
    pub fn end_tick(&mut self) -> Option<(ProfileResults, SaveJob)> {
        self.sample(SamplingPhase::EndTick);
        if !self.kill_switch && get_nanos() <= self.deadline_nano {
            self.single_tick_profiler = Some(self.new_single_tick_profiler());
            return None;
        }
        self.kill_switch = false;
        let results = self.task_profiler.lock().get_results();
        self.single_tick_profiler = None;
        let job = SaveJob { samplers: std::mem::take(&mut self.samplers), deviations: std::mem::take(&mut self.deviations), results: results.clone() };
        Some((results, job))
    }
}

impl SaveJob {
    /// `MetricsPersister("server").saveReports(..)`: the temp directory holding `server/{metrics,deviations,profiling.txt}`.
    pub fn save_reports(self) -> io::Result<PathBuf> {
        std::fs::create_dir_all(PROFILING_RESULTS_DIR)?;
        let temp_dir = create_temp_dir("minecraft-profiling")?;
        let working_dir = temp_dir.join("server");
        save_metrics(&self.samplers, &working_dir.join("metrics"))?;
        if !self.deviations.is_empty() {
            save_deviations(&self.deviations, &working_dir.join("deviations"))?;
        }
        self.results.save_results(&working_dir.join("profiling.txt"));
        Ok(temp_dir)
    }
}

fn create_temp_dir(prefix: &str) -> io::Result<PathBuf> {
    let base = std::env::temp_dir();
    let mut salt = (get_nanos() as u64) ^ u64::from(std::process::id());
    loop {
        let path = base.join(format!("{prefix}{salt}"));
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => salt = salt.wrapping_add(1),
            Err(e) => return Err(e),
        }
    }
}

/// `StringEscapeUtils.escapeCsv`.
fn escape_csv(value: &str) -> String {
    if value.contains([',', '"', '\r', '\n']) { format!("\"{}\"", value.replace('"', "\"\"")) } else { value.to_owned() }
}

fn save_metrics(samplers: &[MetricSampler], dir: &Path) -> io::Result<()> {
    if samplers.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Expected at least one sampler to persist"));
    }
    let mut categories: Vec<MetricCategory> = Vec::new();
    for sampler in samplers {
        if !categories.contains(&sampler.category) {
            categories.push(sampler.category);
        }
    }
    for category in categories {
        let in_category: Vec<&MetricSampler> = samplers.iter().filter(|s| s.category == category).collect();
        let file = dir.join(format!("{}.csv", sanitize_to_path_chars(category.get_description())));
        let write = || -> io::Result<()> {
            std::fs::create_dir_all(dir)?;
            let mut csv = String::from("@tick");
            for sampler in &in_category {
                csv.push(',');
                csv.push_str(&escape_csv(&sampler.name));
            }
            csv.push_str("\r\n");
            let first = in_category.iter().filter_map(|s| s.ticks.first()).min().copied().unwrap_or(0);
            let last = in_category.iter().filter_map(|s| s.ticks.last()).max().copied().unwrap_or(-1);
            for tick in first..=last {
                let _ = write!(csv, "{tick}");
                for sampler in &in_category {
                    csv.push(',');
                    csv.push_str(&escape_csv(&java_double_to_string(sampler.value_at_tick(tick))));
                }
                csv.push_str("\r\n");
            }
            std::fs::write(&file, csv)
        };
        match write() {
            Ok(()) => log::info!("Flushed metrics to {}", file.display()),
            Err(e) => log::error!("Could not save profiler results to {}\n{e}", file.display()),
        }
    }
    Ok(())
}

fn save_deviations(deviations: &[(String, RecordedDeviation)], directory: &Path) -> io::Result<()> {
    for (sampler_name, deviation) in deviations {
        let timestamp = deviation.timestamp.format("%Y-%m-%d_%H.%M.%S%.3f");
        let file = directory.join(sanitize_to_path_chars(sampler_name)).join(format!("{}@{timestamp}.txt", deviation.tick));
        deviation.profiler_result_at_tick.save_results(&file);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_ticks_and_writes_reports() {
        let mut recorder = ActiveMetricsRecorder::create_started(false);
        for _ in 0..3 {
            recorder.start_tick();
            {
                let _scope = super::super::use_profilers(recorder.profilers());
                super::super::push("tick");
                super::super::pop();
            }
            assert!(recorder.end_tick().is_none());
        }
        recorder.end();
        recorder.start_tick();
        {
            let _scope = super::super::use_profilers(recorder.profilers());
        }
        let (results, job) = recorder.end_tick().expect("kill switch ends the recording");
        assert_eq!(results.get_tick_duration(), 4);
        let dir = tempfile::tempdir().unwrap();
        let metrics = dir.path().join("metrics");
        save_metrics(&job.samplers, &metrics).unwrap();
        let csv = std::fs::read_to_string(metrics.join("ticking.csv")).unwrap();
        let mut lines = csv.split("\r\n");
        assert_eq!(lines.next(), Some("@tick,ticktime"));
        assert_eq!(lines.filter(|l| !l.is_empty()).count(), 4);
        let file = dir.path().join("profiling.txt");
        assert!(results.save_results(&file));
        let report = std::fs::read_to_string(file).unwrap();
        assert!(report.contains("Tick span: 4 ticks") && report.contains("--- BEGIN PROFILE DUMP ---"), "{report}");
    }
}
