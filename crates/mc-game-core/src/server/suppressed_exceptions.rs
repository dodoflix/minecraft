//! `SuppressedExceptionCollector`: the latest exceptions the server swallowed and how often each kind happened, shown as
//! the "Suppressed Exceptions" detail of the crash report's system details.

use parking_lot::Mutex;
use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

/// `LATEST_ENTRY_COUNT`.
const LATEST_ENTRY_COUNT: usize = 8;

struct LongEntry {
    timestamp_ms: i64,
    location: String,
    class: String,
    message: String,
}

#[derive(Default)]
struct Entries {
    latest: VecDeque<LongEntry>,
    /// `entryCounts` (`Object2IntLinkedOpenHashMap`): the kind updated last comes first (`putAndMoveToFirst`).
    counts: Vec<((String, String), i32)>,
}

#[derive(Default)]
pub struct SuppressedExceptionCollector {
    entries: Mutex<Entries>,
}

fn current_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

impl SuppressedExceptionCollector {
    /// `addEntry(location, throwable)`: `class` is the throwable's Java class name, `message` its `getMessage()`.
    pub fn add_entry(&self, location: &str, class: &str, message: &str) {
        let mut entries = self.entries.lock();
        entries.latest.push_back(LongEntry {
            timestamp_ms: current_time_ms(),
            location: location.to_owned(),
            class: class.to_owned(),
            message: message.to_owned(),
        });
        while entries.latest.len() > LATEST_ENTRY_COUNT {
            entries.latest.pop_front();
        }
        let key = (location.to_owned(), class.to_owned());
        let count = entries
            .counts
            .iter()
            .position(|(k, _)| *k == key)
            .map_or(0, |i| entries.counts.remove(i).1);
        entries.counts.insert(0, (key, count + 1));
    }

    /// `dump()`: the text of the "Suppressed Exceptions" detail.
    pub fn dump(&self) -> String {
        let entries = self.entries.lock();
        let now = current_time_ms();
        let mut result = String::new();
        if !entries.latest.is_empty() {
            result.push_str("\n\t\tLatest entries:\n");
            for e in &entries.latest {
                // `Class.toString()` prefixes `class `
                result.push_str(&format!(
                    "\t\t\t{}:class {}: {} ({}ms ago)\n",
                    e.location,
                    e.class,
                    e.message,
                    now - e.timestamp_ms
                ));
            }
        }
        if !entries.counts.is_empty() {
            if result.is_empty() {
                result.push('\n');
            }
            result.push_str("\t\tEntry counts:\n");
            for ((location, class), count) in &entries.counts {
                result.push_str(&format!("\t\t\t{location}:class {class} x {count}\n"));
            }
        }
        if result.is_empty() {
            "~~NONE~~".to_owned()
        } else {
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_lists_the_latest_entries_and_the_counts() {
        let collector = SuppressedExceptionCollector::default();
        assert_eq!(collector.dump(), "~~NONE~~");
        collector.add_entry("chunk/load", "java.io.IOException", "a");
        collector.add_entry("chunk/save", "java.io.IOException", "b");
        collector.add_entry("chunk/load", "java.io.IOException", "c");
        let dump = collector.dump();
        assert!(
            dump.starts_with(
                "\n\t\tLatest entries:\n\t\t\tchunk/load:class java.io.IOException: a ("
            )
        );
        assert!(dump.ends_with(
            "\t\tEntry counts:\n\t\t\tchunk/load:class java.io.IOException x 2\n\t\t\tchunk/save:class java.io.IOException x 1\n"
        ));
        for i in 0..10 {
            collector.add_entry("packet/x", "java.lang.Error", &i.to_string());
        }
        assert_eq!(
            collector.dump().matches("ms ago)").count(),
            LATEST_ENTRY_COUNT
        );
    }
}
