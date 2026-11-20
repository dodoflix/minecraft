//! Replay of the corpus fixtures (`fixtures/samples/corpus_*.tsv.gz`, written by `scripts/datafix-corpus.sh`): inputs
//! read from the worlds of old vanilla servers or generated from the schemas, and what vanilla's `DataFixers` answered for
//! each target version. A fixture is the gzip of tab separated lines:
//!
//! ```text
//! S  ID  TYPE  FROM  BASE64                                         a sample (the NbtIo bytes of its input)
//! R  ID  TO  STATUS  SHA256  EXCEPTION  EXCEPTION_HASH  ERRORS  ERRORS_HASH   vanilla's answer for the update to TO
//! ```
//!
//! Every answer is compared with ours by four rules: the `NbtIo` bytes of the result (equal bytes: equal tree and equal
//! key order), the exception class and message hash, and the ERROR log events (count and message hash). An update that
//! crosses a fix our chain does not register yet (the rows of `chain_expected_missing.tsv`) is skipped, and runs from
//! the commit that registers the fix.

use crate::compare::{Outcome, Request, Status, run_ours, sha256_hex};
use crate::recorded_chain::{Row, parse};
use mc_datafix::data_fixer;
use std::collections::HashMap;

const NOT_REGISTERED: &str = include_str!("../fixtures/chain_expected_missing.tsv");

/// What replaying a fixture did.
#[derive(Default, Debug)]
pub struct Report {
    pub ran: usize,
    /// Updates skipped for a fix of the chain that is not registered yet.
    pub skipped: usize,
    pub failures: Vec<String>,
}

/// The fixes of vanilla's chain not registered yet that an update from `from` to `to` runs (`DataFixerUpper.getRule`:
/// every fix above the newest fix at or below `from`, up to the schema version `to`).
fn missing_fixes_between(
    recorded: &[Row],
    not_registered: &[Row],
    from: i32,
    to: i32,
) -> Vec<String> {
    let keys: Vec<i32> = recorded
        .iter()
        .filter(|r| r.kind == "data")
        .map(|r| r.version * 10 + r.sub)
        .collect();
    let from_key = from.saturating_mul(10);
    let floor = match keys.iter().copied().filter(|k| *k <= from_key).max() {
        Some(k) => k,
        None => keys.iter().copied().min().map_or(i32::MIN, |k| k - 1),
    };
    not_registered
        .iter()
        .filter(|r| r.kind == "data" && r.version * 10 + r.sub > floor && r.version <= to)
        .map(|r| format!("{} at {}", r.class, r.version))
        .collect()
}

/// `None` when ours equals vanilla's answer, else what differs.
fn difference(ours: &Outcome, vanilla: &[&str]) -> Option<String> {
    let [status, sha, exc, exc_hash, errs, errs_hash] = vanilla[..] else {
        panic!("malformed answer: {vanilla:?}");
    };
    let describe = |s: &Status| match s {
        Status::Ok { .. } => "a result".to_string(),
        Status::Threw { class, .. } => format!("threw {class}"),
        Status::Bad(why) => format!("an unreadable input ({why})"),
    };
    let same = match (&ours.status, status) {
        (Status::Ok { bytes, .. }, "ok") => {
            let ours = sha256_hex(bytes);
            if ours != sha {
                return Some(format!(
                    "result bytes differ (sha256 vanilla {sha}, ours {ours})"
                ));
            }
            true
        }
        (
            Status::Threw {
                class,
                message_hash,
            },
            "threw",
        ) => {
            if class != exc {
                return Some(format!("exception class: vanilla {exc}, ours {class}"));
            }
            if message_hash != exc_hash {
                return Some(format!(
                    "exception message hash: vanilla {exc_hash}, ours {message_hash}"
                ));
            }
            true
        }
        (Status::Bad(_), "bad") => true,
        _ => false,
    };
    if !same {
        return Some(format!(
            "vanilla answered {status}, ours {}",
            describe(&ours.status)
        ));
    }
    let count: usize = errs.parse().expect("error count");
    if count != ours.error_count {
        return Some(format!(
            "ERROR log events: vanilla {count}, ours {}",
            ours.error_count
        ));
    }
    let hash = (errs_hash != "-").then_some(errs_hash);
    if hash != ours.errors_hash.as_deref() {
        return Some(format!(
            "ERROR log message hash: vanilla {errs_hash}, ours {}",
            ours.errors_hash.as_deref().unwrap_or("-")
        ));
    }
    None
}

/// Replays every answer of `fixture` (the decompressed text).
pub fn replay(fixture: &str) -> Report {
    let recorded = parse(include_str!("../fixtures/fixes.tsv"));
    let not_registered = parse(NOT_REGISTERED);
    let fixer = data_fixer();
    let mut report = Report::default();
    // the samples stay encoded and the updates run grouped by version range: the fixer keeps the rule of a range it
    // has seen, so the cache is dropped when the next range starts
    let mut samples: HashMap<&str, (&str, i32, &str)> = HashMap::new();
    let mut answers: Vec<(i32, i32, &str, Vec<&str>)> = Vec::new();
    for line in fixture.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split('\t').collect();
        match f[..] {
            ["S", id, ty, from, b64] => {
                samples.insert(id, (ty, from.parse().expect("from"), b64));
            }
            ["R", id, to, ref answer @ ..] => {
                let (_, from, _) = samples.get(id).expect("an answer follows its sample");
                answers.push((*from, to.parse().expect("to"), id, answer.to_vec()));
            }
            _ => panic!("malformed fixture line: {line:.80}"),
        }
    }
    answers.sort_by(|a, b| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)));
    let mut range = None;
    for (from, to, id, answer) in answers {
        if !missing_fixes_between(&recorded, &not_registered, from, to).is_empty() {
            report.skipped += 1;
            continue;
        }
        if range != Some((from, to)) {
            fixer.clear_rule_cache();
            range = Some((from, to));
        }
        let (ty, _, b64) = samples[id];
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64)
            .expect("sample bytes are base64");
        let input = crate::compare::read_tag(&bytes)
            .unwrap_or_else(|e| panic!("sample {id} is not NBT: {e}"));
        let request = Request {
            id: id.to_string(),
            ty: ty.to_string(),
            from,
            to,
            input,
        };
        report.ran += 1;
        if let Some(d) = difference(&run_ours(fixer, &request), &answer) {
            report.failures.push(format!("{id} {from} -> {to}: {d}"));
        }
    }
    report
}

/// `replay`, failing with the first differences; `answers` is the number of answers the fixture holds.
pub fn assert_replays(fixture: &str, answers: usize) {
    let report = replay(fixture);
    assert_eq!(
        report.ran + report.skipped,
        answers,
        "the fixture holds another number of answers"
    );
    assert!(
        report.failures.is_empty(),
        "{} of {} replayed updates differ ({} skipped for fixes not registered yet):\n{}",
        report.failures.len(),
        report.ran,
        report.skipped,
        report
            .failures
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(version: i32, sub: i32, class: &str) -> Row {
        Row {
            index: 0,
            version,
            sub,
            kind: "data".into(),
            class: class.into(),
        }
    }

    #[test]
    fn an_update_meets_the_unregistered_fixes_above_its_version_up_to_its_target() {
        let recorded = [
            row(100, 0, "A"),
            row(500, 0, "B"),
            row(500, 1, "C"),
            row(900, 0, "D"),
        ];
        let missing = [row(500, 0, "B"), row(500, 1, "C")];
        let meets = |from, to| missing_fixes_between(&recorded, &missing, from, to).len();
        assert_eq!(meets(-1, 499), 0);
        assert_eq!(meets(-1, 500), 2);
        assert_eq!(meets(499, 900), 2);
        assert_eq!(
            meets(500, 900),
            1,
            "a world at 500 has met 500.0 but not 500.1"
        );
        assert_eq!(meets(501, 900), 0);
    }
}
