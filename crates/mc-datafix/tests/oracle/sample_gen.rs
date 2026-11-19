//! The schema-driven sample generator (`scripts/datafix-oracle.sh gen`, `tests/oracle/SampleGen.java`): conformance to the
//! schemas, determinism, the error path, mutation and the matrix modes.

use crate::compare::Status;
use crate::oracle_client::{oracle, run_verb};
use base64::Engine;
use std::collections::BTreeSet;
use std::path::PathBuf;

const VERSIONS: &str = "99,1343,1519,2730,3465,3955,4671,5016";

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("datafix-oracle");
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir.join(name)
}

fn path(p: &std::path::Path) -> &str {
    p.to_str().expect("utf-8 path")
}

/// One `gen` run: the batch lines, and the report rows split into their columns.
fn samples(args: &[&str], report: &str) -> (String, Vec<Vec<String>>) {
    let file = scratch(report);
    let mut full = vec!["gen"];
    full.extend_from_slice(args);
    full.extend_from_slice(&["--report", path(&file)]);
    let lines = run_verb(&full);
    let rows = std::fs::read_to_string(&file)
        .expect("the report")
        .lines()
        .map(|l| l.split('\t').map(str::to_string).collect())
        .collect();
    (lines, rows)
}

fn update(line: &str) -> crate::compare::Outcome {
    let id = line.split('\t').next().expect("id");
    oracle().exchange(id, &format!("{line}\n"))
}

#[test]
#[ignore = "oracle: runs vanilla's DataFixers in a JVM"]
fn every_type_and_version_yields_readable_samples() {
    let (lines, rows) = samples(
        &["--type", "all", "--from", VERSIONS, "--n", "20"],
        "gen-all.tsv",
    );
    let mut absent = BTreeSet::new();
    let mut emitted = 0;
    for row in &rows {
        let [ty, from, _to, state, n, first_ok, kept, ..] = row.as_slice() else {
            panic!("malformed report row {row:?}");
        };
        if state == "absent" {
            absent.insert(format!("{ty}@{from}"));
            continue;
        }
        assert_eq!(state, "ok", "{row:?}");
        let (n, first_ok): (usize, usize) = (n.parse().unwrap(), first_ok.parse().unwrap());
        assert!(
            first_ok * 10 >= n * 9,
            "only {first_ok} of {n} samples of {ty}@{from} read on the first try"
        );
        emitted += kept.parse::<usize>().unwrap();
    }
    // the types vanilla registers in a later schema do not exist at 99
    assert_eq!(
        absent.into_iter().collect::<Vec<_>>(),
        ["ADVANCEMENTS@99", "HOTBAR@99"]
    );
    assert_eq!(emitted, lines.lines().count());
    assert_eq!(emitted, 246 * 20);
}

#[test]
#[ignore = "oracle: runs vanilla's DataFixers in a JVM"]
fn the_output_is_a_function_of_the_arguments() {
    let args = ["--type", "CHUNK,PLAYER", "--from", "1343,3955", "--n", "5"];
    let (first, _) = samples(&args, "det-a.tsv");
    let (second, _) = samples(&args, "det-b.tsv");
    assert!(first == second, "two runs with the same seed differ");
    let mut other = args.to_vec();
    other.extend_from_slice(&["--seed", "2"]);
    assert!(
        samples(&other, "det-c.tsv").0 != first,
        "the seed has no effect"
    );
}

#[test]
#[ignore = "oracle: runs vanilla's DataFixers in a JVM"]
fn samples_update_in_vanilla_and_damaged_ones_take_the_error_path() {
    let args = ["--type", "CHUNK,PLAYER,LEVEL", "--from", "1343,3955"];
    let (clean, _) = samples(&[&args[..], &["--n", "20"]].concat(), "upd-clean.tsv");
    let clean: Vec<_> = clean.lines().collect();
    let ok = clean
        .iter()
        .filter(|l| matches!(update(l).status, Status::Ok { .. }))
        .count();
    assert!(
        ok * 10 >= clean.len() * 9,
        "{ok} of {} updated",
        clean.len()
    );

    let (damaged, rows) = samples(
        &[&args[..], &["--n", "20", "--invalid", "1"]].concat(),
        "upd-bad.tsv",
    );
    assert!(
        rows.iter().filter(|r| r[3] == "ok").all(|r| r[8] == "20"),
        "every sample is damaged"
    );
    let logged = damaged
        .lines()
        .filter(|l| update(l).error_count > 0)
        .count();
    // most schemas read leniently (optional fields, passthrough), so only part of the damage breaks the read: vanilla logs an
    // ERROR and returns the input, which the log appender of the oracle captures
    assert!(
        logged >= 10,
        "only {logged} damaged samples logged an ERROR"
    );
}

#[test]
#[ignore = "oracle: runs vanilla's DataFixers in a JVM"]
fn mutants_follow_the_input_and_stay_deterministic() {
    let (real, _) = samples(
        &["--type", "CHUNK", "--from", "1343", "--n", "10"],
        "mut-in.tsv",
    );
    let input = scratch("mut-in.tsv.batch");
    std::fs::write(&input, &real).expect("write the samples");
    let args = ["--mutate", path(&input), "--n", "3", "--seed", "5"];
    let (mutants, _) = samples(&args, "mut-a.tsv");
    assert_eq!(mutants.lines().count(), 30);
    assert!(samples(&args, "mut-b.tsv").0 == mutants);
    let originals: Vec<&str> = real
        .lines()
        .map(|l| l.rsplit('\t').next().unwrap())
        .collect();
    let mut changed = 0;
    for (i, m) in mutants.lines().enumerate() {
        let f: Vec<&str> = m.split('\t').collect();
        assert!(f[0].starts_with(real.lines().nth(i / 3).unwrap().split('\t').next().unwrap()));
        assert_eq!((f[1], f[2], f[3]), ("CHUNK", "1343", "5023"));
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(f[4])
            .expect("base64");
        crate::compare::read_tag(&bytes).expect("a mutant is NBT");
        changed += usize::from(f[4] != originals[i / 3]);
    }
    assert!(changed >= 24, "only {changed} of 30 mutants differ");
}

#[test]
#[ignore = "oracle: runs vanilla's DataFixers in a JVM"]
fn matrix_shards_partition_the_groups_and_random_is_sorted() {
    let all = samples(&["--matrix", "chain", "--n", "1"], "m-all.tsv").0;
    let mut parts: Vec<String> = (0..3)
        .flat_map(|i| {
            samples(
                &[
                    "--matrix",
                    "chain",
                    "--n",
                    "1",
                    "--shard",
                    &format!("{i}/3"),
                ],
                "m-part.tsv",
            )
            .0
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>()
        })
        .collect();
    let mut whole: Vec<String> = all.lines().map(str::to_string).collect();
    parts.sort();
    whole.sort();
    assert!(!whole.is_empty() && parts == whole);

    let (random, rows) = samples(
        &["--matrix", "random", "--n", "300", "--seed", "3"],
        "m-random.tsv",
    );
    let keys: Vec<(String, i32, i32)> = rows
        .iter()
        .map(|r| (r[0].clone(), r[1].parse().unwrap(), r[2].parse().unwrap()))
        .collect();
    assert!(keys.windows(2).all(|w| w[0] <= w[1]), "groups are sorted");
    assert!(
        keys.iter().any(|k| k.2 < k.1),
        "to below from (a no-op) occurs"
    );
    assert!(
        keys.iter().any(|k| k.2 < 5023 && k.2 > k.1),
        "partial ranges occur"
    );
    assert!(
        keys.iter().any(|k| k.1 <= 100),
        "from below the first schema occurs"
    );
    assert!(random.lines().count() > 250);
}

#[test]
#[ignore = "oracle: runs vanilla's DataFixers in a JVM"]
fn a_root_that_is_sometimes_not_a_compound_still_yields_its_compound_samples() {
    // from schema 4290 on a text component is a string, a list or a compound
    let (lines, rows) = samples(
        &[
            "--type",
            "ref:text_component",
            "--from",
            "4657",
            "--n",
            "10",
        ],
        "mixed-root.tsv",
    );
    assert_eq!(rows[0][3], "ok", "{rows:?}");
    assert_eq!(lines.lines().count(), 10);
}
