//! `net.minecraft.util.profiling`: the section profiler (`ActiveProfiler`, `Profiler`), its results
//! (`FilledProfileResults`) and the metrics recorder behind `/perf` (`metrics`).
//!
//! `Profiler.get()` is a thread local in vanilla; here [`push`]/[`pop`]/[`pop_push`]/[`increment_counter`] act on the
//! profilers installed on the calling thread by [`use_profilers`] and do nothing when none is.

mod active_profiler;
pub mod jfr;
pub mod metrics;
mod results;

use std::cell::RefCell;
use std::sync::Arc;

use parking_lot::Mutex;

pub use active_profiler::ActiveProfiler;
pub use results::{FilledProfileResults, ProfileResults, ResultField};

/// A profiler shared between the thread-local `Profiler.get()` and its owner (the metrics recorder).
pub type ProfilerHandle = Arc<Mutex<ActiveProfiler>>;

/// `MetricCategory` (`description` names the CSV file).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MetricCategory {
    PathFinding,
    EventLoops,
    ConsecutiveExecutors,
    TickLoop,
    Jvm,
    ChunkRendering,
    ChunkRenderingDispatching,
    Cpu,
    Gpu,
}

impl MetricCategory {
    pub fn get_description(self) -> &'static str {
        match self {
            Self::PathFinding => "pathfinding",
            Self::EventLoops => "event-loops",
            Self::ConsecutiveExecutors => "consecutive-executors",
            Self::TickLoop => "ticking",
            Self::Jvm => "jvm",
            Self::ChunkRendering => "chunk rendering",
            Self::ChunkRenderingDispatching => "chunk rendering dispatching",
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
        }
    }
}

thread_local! {
    static ACTIVE: RefCell<Option<Vec<ProfilerHandle>>> = const { RefCell::new(None) };
}

/// `Profiler.Scope`: `Profiler.use(filler)`; dropping it ends the tick of every profiler.
pub struct ProfilerScope(bool);

/// `Profiler.use(combine(profilers))`: starts the tick of each profiler and makes them the thread's `Profiler.get()`.
pub fn use_profilers(profilers: Vec<ProfilerHandle>) -> ProfilerScope {
    if profilers.is_empty() {
        return ProfilerScope(false);
    }
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        assert!(active.is_none(), "Profiler is already active");
        for profiler in &profilers {
            profiler.lock().start_tick();
        }
        *active = Some(profilers);
    });
    ProfilerScope(true)
}

impl Drop for ProfilerScope {
    fn drop(&mut self) {
        if self.0 {
            let profilers = ACTIVE.with(|active| active.borrow_mut().take()).expect("Profiler was not active");
            for profiler in &profilers {
                profiler.lock().end_tick();
            }
        }
    }
}

fn each(f: impl Fn(&mut ActiveProfiler)) {
    ACTIVE.with(|active| {
        if let Some(profilers) = active.borrow().as_ref() {
            for profiler in profilers {
                f(&mut profiler.lock());
            }
        }
    });
}

/// `Profiler.get().push(name)`.
pub fn push(name: &str) {
    each(|p| p.push(name));
}

/// `Profiler.get().push(() -> name)`: the name is only built when a profiler is active.
pub fn push_with(name: impl Fn() -> String) {
    each(|p| p.push(&name()));
}

/// `Profiler.get().pop()`.
pub fn pop() {
    each(|p| p.pop());
}

/// `Profiler.get().popPush(name)`.
pub fn pop_push(name: &str) {
    each(|p| p.pop_push(name));
}

/// `Profiler.get().incrementCounter(name)`.
pub fn increment_counter(name: &str) {
    each(|p| p.increment_counter(name, 1));
}
