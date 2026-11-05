//! `MinecraftServer`'s metrics recording (`startRecordingMetrics`, `finishRecordingMetrics`, `createProfiler`,
//! `endMetricsRecordingTick`, `saveDebugReport`): the server side of `/perf`.

use std::fmt::Write as _;
use std::io;
use std::path::Path;

use super::MinecraftServer;
use crate::commands::server_access::enter_server;
use crate::util::profiling::metrics::ActiveMetricsRecorder;
use crate::util::profiling::{self, ProfileResults, ProfilerScope};

/// `onMetricsRecordingStopped` (runs on the game thread).
type StoppedCallback = Box<dyn FnOnce(ProfileResults) + Send>;
/// `onMetricsRecordingFinished`: called with the temp report directory on the IO thread.
type FinishedCallback = Box<dyn FnOnce(&Path) + Send>;

#[derive(Default)]
pub struct MetricsRecording {
    recorder: Option<ActiveMetricsRecorder>,
    will_start: bool,
    on_stopped: Option<StoppedCallback>,
    on_finished: Option<FinishedCallback>,
}

impl MinecraftServer {
    /// `isRecordingMetrics()`.
    pub fn is_recording_metrics(&self) -> bool {
        self.metrics_recording.recorder.is_some()
    }

    /// `startRecordingMetrics(onStopped, onFinished)`: the recorder starts with the next tick.
    pub fn start_recording_metrics(&mut self, on_stopped: impl FnOnce(ProfileResults) + Send + 'static, on_finished: impl FnOnce(&Path) + Send + 'static) {
        self.metrics_recording.on_stopped = Some(Box::new(on_stopped));
        self.metrics_recording.on_finished = Some(Box::new(on_finished));
        self.metrics_recording.will_start = true;
    }

    /// `finishRecordingMetrics()`.
    pub fn finish_recording_metrics(&mut self) {
        if let Some(recorder) = &mut self.metrics_recording.recorder {
            recorder.end();
        }
    }

    /// `cancelRecordingMetrics()`: the stop callback gets `EmptyProfileResults.EMPTY`, nothing is saved.
    pub(super) fn cancel_recording_metrics(&mut self) {
        if self.metrics_recording.recorder.take().is_some() {
            self.metrics_recording.on_finished = None;
            if let Some(on_stopped) = self.metrics_recording.on_stopped.take() {
                enter_server(self, || on_stopped(ProfileResults::Empty));
            }
        }
    }

    /// `createProfiler()` + `Profiler.use(..)`: starts the recorder when requested, starts its tick and installs its
    /// profilers on this thread until the scope is dropped.
    pub(super) fn begin_metrics_tick(&mut self) -> ProfilerScope {
        if self.metrics_recording.will_start {
            self.metrics_recording.recorder = Some(ActiveMetricsRecorder::create_started(true));
            self.metrics_recording.will_start = false;
        }
        match &mut self.metrics_recording.recorder {
            Some(recorder) => {
                recorder.start_tick();
                profiling::use_profilers(recorder.profilers())
            }
            None => profiling::use_profilers(Vec::new()),
        }
    }

    /// `endMetricsRecordingTick()`: samples; when the recording ended (`/perf stop` or 10 s) reports its results and
    /// persists the report on an IO thread.
    pub(super) fn end_metrics_recording_tick(&mut self) {
        let Some(recorder) = &mut self.metrics_recording.recorder else { return };
        let Some((results, job)) = recorder.end_tick() else { return };
        // `onProfilingEnd`: `stopRecordingMetrics()` then the command's callback.
        self.metrics_recording.recorder = None;
        if let Some(on_stopped) = self.metrics_recording.on_stopped.take() {
            enter_server(self, || on_stopped(results));
        }
        // `scheduleSaveResults`
        let on_finished = self.metrics_recording.on_finished.take();
        let shared = self.shared.clone();
        let spawned = std::thread::Builder::new().name("IO-Worker-metrics".into()).spawn(move || {
            let report_path = match job.save_reports() {
                Ok(path) => path,
                Err(e) => {
                    log::error!("Failed to save the profiling reports\n{e}");
                    return;
                }
            };
            let output = report_path.join("server");
            shared.execute_blocking({
                let output = output.clone();
                move |server| server.save_debug_report(&output)
            });
            if let Some(on_finished) = on_finished {
                on_finished(&report_path);
            }
        });
        if let Err(e) = spawned {
            log::error!("Failed to start the profiling report thread: {e}");
        }
    }

    /// `saveDebugReport(output)`. Not written: `levels/` (`ServerLevel.saveDebugReport`), `classpath.txt`,
    /// `threads.txt` and `modules.txt` (JVM introspection).
    fn save_debug_report(&mut self, output: &Path) {
        let write = || -> io::Result<()> {
            std::fs::create_dir_all(output)?;
            self.dump_game_rules(&output.join("gamerules.txt"))?;
            self.dump_misc_stats(&output.join("stats.txt"))?;
            self.dump_server_properties(&output.join("server.properties.txt"))
        };
        if let Err(e) = write() {
            log::warn!("Failed to save debug report\n{e}");
        }
    }

    fn dump_game_rules(&self, path: &Path) -> io::Result<()> {
        struct Collect(Vec<crate::world::level::gamerules::AnyGameRule>);
        impl crate::world::level::gamerules::GameRuleTypeVisitor for Collect {
            fn visit(&mut self, rule: crate::world::level::gamerules::AnyGameRule) {
                self.0.push(rule);
            }
        }
        let rules = self.game_rules_ref();
        let mut collect = Collect(Vec::new());
        rules.visit_game_rule_types(&mut collect);
        let mut out = String::new();
        for rule in collect.0 {
            let _ = writeln!(out, "{}={}", rule.get_identifier(), rules.get_as_string(rule));
        }
        std::fs::write(path, out)
    }

    fn dump_misc_stats(&mut self, path: &Path) -> io::Result<()> {
        let tick_times: Vec<String> = self.get_tick_times_nanos().iter().map(i64::to_string).collect();
        let mut out = String::new();
        let _ = writeln!(out, "pending_tasks: {}", self.pending_tasks_count());
        let _ = writeln!(out, "average_tick_time: {}", crate::util::java_format::format_fixed(f64::from(self.get_current_smoothed_tick_time()), 6));
        let _ = writeln!(out, "tick_times: [{}]", tick_times.join(", "));
        std::fs::write(path, out)
    }

    /// `DedicatedServer.dumpServerProperties`.
    fn dump_server_properties(&self, path: &Path) -> io::Result<()> {
        let p = self.properties();
        let mut out = String::new();
        let _ = writeln!(out, "sync-chunk-writes={}", p.sync_chunk_writes);
        let _ = writeln!(out, "gamemode={}", p.game_mode.get().get_name().to_uppercase());
        let _ = writeln!(out, "entity-broadcast-range-percentage={}", p.entity_broadcast_range_percentage.get());
        let _ = writeln!(out, "max-world-size={}", p.max_world_size);
        let _ = writeln!(out, "view-distance={}", p.view_distance.get());
        let _ = writeln!(out, "simulation-distance={}", p.simulation_distance.get());
        let _ = writeln!(out, "generate-structures={}", p.world_options.generate_structures);
        let _ = writeln!(out, "use-native={}", p.use_native_transport);
        let _ = writeln!(out, "rate-limit={}", p.rate_limit_packets_per_second);
        let _ = writeln!(out, "command-spam-threshold-seconds={}", p.command_spam_threshold_seconds);
        let _ = writeln!(out, "chat-spam-threshold-seconds={}", p.chat_spam_threshold_seconds);
        std::fs::write(path, out)
    }
}
