//! Rules of vanilla's worldgen executor that hold for every run whatever the thread timing, checked on the scheduler trace
//! `mc-server` writes when `MC_WORLDGEN_SCHED_TRACE` names a file (`chunk_status_tasks::sched_trace`). A trace that passes is
//! a schedule vanilla's executor can produce, which is the bar for the order chunks are decorated in (the real server's own
//! order varies between runs: how many resubmissions precede a poll of the dispatcher depends on thread timing).
//!
//! * R1 queue discipline: every `POP` names the first chunk of the lowest non-empty level of the `ChunkTaskPriorityQueue`
//!   replayed from the `DSUB` (insert) and `RESORT` (level change moves the tasks to the end of the new level) events.
//! * R4 no worldgen-thread step (`S`) runs before the first poll.
//! * R5 slice granularity: a task slice (`runUntilWait`) ends waiting only on a step whose future can still be pending
//!   when the loop checks it: empty (chunk load), biomes and terrain (background executor), initialize_light and light (light
//!   engine), full (game thread). Structure starts and references, features and spawn complete inside the slice, so a
//!   slice that queues itself behind other chunks' tasks on one of them (`cont=0`) is an order vanilla cannot produce.
//!
//! The vanilla dispatcher's inbox rules (a poll never runs while a submit waits, submits are FIFO) have no counterpart:
//! `mc-server` inserts into the queue where the future completes.

use std::collections::BTreeMap;

type Pos = (i32, i32);

/// What [`check`] found in one trace.
#[derive(Debug, Default)]
pub(crate) struct Report {
    pub(crate) polls: usize,
    pub(crate) slices: usize,
    pub(crate) violations: Vec<String>,
}

const PENDING_CAPABLE: &[&str] = &[
    "empty",
    "biomes",
    "terrain",
    "initialize_light",
    "light",
    "full",
];

fn pos(text: &str) -> Option<Pos> {
    let (x, z) = text.split_once(',')?;
    Some((x.parse().ok()?, z.parse().ok()?))
}

/// The queue level of one chunk's tasks: first insertion position matters, so a list keeps the order.
#[derive(Default)]
struct Queue {
    levels: BTreeMap<u32, Vec<Pos>>,
}

impl Queue {
    fn insert(&mut self, level: u32, at: Pos) {
        let entries = self.levels.entry(level).or_default();
        if !entries.contains(&at) {
            entries.push(at);
        }
    }

    fn resort(&mut self, old: u32, new: u32, at: Pos) {
        if let Some(entries) = self.levels.get_mut(&old)
            && let Some(i) = entries.iter().position(|p| *p == at)
        {
            entries.remove(i);
            self.insert(new, at);
        }
    }

    /// The head of the lowest non-empty level.
    fn head(&self) -> Option<(u32, Pos)> {
        self.levels
            .iter()
            .find_map(|(level, entries)| entries.first().map(|p| (*level, *p)))
    }

    fn remove(&mut self, at: Pos) {
        for entries in self.levels.values_mut() {
            if let Some(i) = entries.iter().position(|p| *p == at) {
                entries.remove(i);
                return;
            }
        }
    }
}

/// Replays a trace against R1, R4 and R5. A line is `[dimension] EVENT ...` (the dimension prefix says which dimension's
/// queue the event belongs to; a line without one belongs to a single anonymous queue); the events are `DSUB x,z Lq`,
/// `RESORT x,z old->new`, `POP x,z`, `W slice awaited status cont=n`, `E x,z` and `S status x,z`.
pub(crate) fn check(trace: &str) -> Report {
    let mut report = Report::default();
    let mut queues: BTreeMap<&str, (Queue, usize)> = BTreeMap::new();
    for (n, line) in trace.lines().enumerate() {
        let n = n + 1;
        let mut words: Vec<&str> = line.split_whitespace().collect();
        let dimension = if words.first().is_some_and(|w| w.contains(':')) {
            words.remove(0)
        } else {
            ""
        };
        let (queue, polls) = queues.entry(dimension).or_default();
        match words.as_slice() {
            ["DSUB", at, level] => {
                if let (Some(at), Some(level)) = (pos(at), level.strip_prefix('L')) {
                    queue.insert(level.parse().unwrap_or(u32::MAX), at);
                }
            }
            ["RESORT", at, change] => {
                if let (Some(at), Some((old, new))) = (pos(at), change.split_once("->")) {
                    queue.resort(
                        old.parse().unwrap_or(u32::MAX),
                        new.parse().unwrap_or(u32::MAX),
                        at,
                    );
                }
            }
            ["POP", at] => {
                let Some(at) = pos(at) else { continue };
                *polls += 1;
                report.polls += 1;
                match queue.head() {
                    None => report.violations.push(format!(
                        "R1 line {n} {dimension}: poll of {at:?} on an empty queue"
                    )),
                    Some((level, head)) if head != at => report.violations.push(format!(
                        "R1 line {n} {dimension}: polled {at:?}, the head of level {level} is {head:?}"
                    )),
                    Some(_) => {}
                }
                queue.remove(at);
            }
            ["S", status, at] => {
                if *polls == 0 {
                    report.violations.push(format!(
                        "R4 line {n} {dimension}: {status} of {at} before the first poll"
                    ));
                }
            }
            ["W", slice, awaited, status, cont] => {
                let status = status.strip_prefix("minecraft:").unwrap_or(status);
                report.slices += usize::from(*cont == "cont=0");
                if *cont == "cont=0" && !PENDING_CAPABLE.contains(&status) {
                    report.violations.push(format!(
                        "R5 line {n} {dimension}: slice of {slice} ends waiting on {status} of {awaited}"
                    ));
                }
            }
            ["E", _] => report.slices += 1,
            _ => {}
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::check;

    #[test]
    fn a_valid_schedule_passes() {
        let trace = "DSUB 0,0 L33\nDSUB 1,0 L33\nDSUB 2,0 L32\nRESORT 1,0 33->31\nPOP 1,0\nS features 1,0\n\
                     W 1,0 1,0 minecraft:light cont=0\nPOP 2,0\nW 2,0 3,0 features cont=1\nPOP 0,0\nE 0,0\n";
        let report = check(trace);
        assert_eq!(report.violations, Vec::<String>::new());
        assert_eq!((report.polls, report.slices), (3, 2));
    }

    #[test]
    fn a_trace_of_the_server_generating_the_end_passes() {
        // the end of a flat corpus world as `mc-server` generated it while starting (MC_WORLDGEN_SCHED_TRACE)
        let report = check(include_str!("../fixtures/sched_trace_the_end.txt"));
        assert_eq!(report.violations, Vec::<String>::new());
        assert!(report.polls > 300 && report.slices > 300, "{report:?}");
    }

    #[test]
    fn the_queues_of_two_dimensions_are_replayed_apart() {
        let trace = "minecraft:overworld DSUB 0,0 L33\nminecraft:the_nether DSUB 5,5 L33\n\
                     minecraft:the_nether POP 5,5\nminecraft:overworld POP 0,0\nminecraft:the_end S features 0,0\n";
        let report = check(trace);
        assert_eq!(report.violations.len(), 1);
        assert!(report.violations[0].starts_with("R4 line 5 minecraft:the_end"));
    }

    #[test]
    fn a_resort_to_a_level_with_the_chunk_keeps_its_place() {
        // moving 2,0 into level 31 that already holds it keeps the older position
        let trace =
            "DSUB 1,0 L31\nDSUB 2,0 L33\nDSUB 2,0 L31\nRESORT 2,0 33->31\nPOP 1,0\nPOP 2,0\n";
        assert!(check(trace).violations.is_empty());
    }

    #[test]
    fn polling_behind_the_head_breaks_the_queue_discipline() {
        let trace = "DSUB 0,0 L33\nDSUB 1,0 L32\nPOP 0,0\n";
        assert_eq!(check(trace).violations.len(), 1);
        assert!(check(trace).violations[0].starts_with("R1 line 3"));
        assert!(check("POP 0,0\n").violations[0].contains("empty queue"));
    }

    #[test]
    fn a_step_before_the_first_poll_is_flagged() {
        let report = check("S features 0,0\nDSUB 0,0 L33\nPOP 0,0\n");
        assert!(report.violations[0].starts_with("R4"));
    }

    #[test]
    fn a_slice_that_queues_itself_on_an_inline_step_is_flagged() {
        let report =
            check("W 0,0 1,0 minecraft:features cont=0\nW 0,0 1,0 minecraft:features cont=1\n");
        assert_eq!(report.violations.len(), 1);
        assert!(report.violations[0].starts_with("R5 line 1"));
    }
}
