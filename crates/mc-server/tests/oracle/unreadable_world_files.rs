//! An old world whose files or folders the server may not read, list or write: the real 26.3 server and `mc-server`
//! each upgrade a copy of it, and the warnings and errors of the two logs (with the first line of the exception behind
//! each) are compared in order. The file fixer reads, lists, copies and moves the real files, and the exception of a
//! failed operation names the path. The chunks of a folder that cannot be read fail in the hash order of the structure
//! data, and the `Failed to read chunk` lines of the two logs are compared with their positions. Run with
//! `cargo test -p mc-server --test oracle unreadable_ -- --ignored --nocapture`.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::startup_gates::gate_log;
use crate::world_start::{Run, Side, cache_dir, copy_dir, first_log_difference, run_limited};

/// The old-layout world the cases start from (`scripts/datafix-worlds.sh 1.12.2`).
fn old_world() -> std::path::PathBuf {
    let dir = cache_dir().join("worlds/1.12.2-normal");
    assert!(
        dir.join("level.dat").is_file(),
        "{}: no corpus world: run scripts/datafix-worlds.sh 1.12.2",
        dir.display()
    );
    dir
}

/// Gives every file and folder below `dir` its normal mode back, so the temporary directory can be removed.
fn restore(dir: &Path) {
    let Ok(metadata) = fs::symlink_metadata(dir) else {
        return;
    };
    if metadata.is_dir() {
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o755));
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            restore(&entry.path());
        }
    } else {
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o644));
    }
}

fn chmod(world: &Path, path: &str, mode: u32) {
    fs::set_permissions(world.join(path), fs::Permissions::from_mode(mode)).expect("chmod");
}

/// An `upgrade_in_progress.json` of an earlier start that holds `bytes`.
fn journal(world: &Path, bytes: &[u8], mode: u32) {
    fs::create_dir_all(world.join("filefix")).expect("filefix folder");
    fs::write(world.join("filefix/upgrade_in_progress.json"), bytes).expect("journal");
    chmod(world, "filefix/upgrade_in_progress.json", mode);
}

/// A case: what is wrong with the files of the copy of the old world.
type Damage = fn(&Path);

const CASES: [(&str, Damage); 36] = [
    ("saved_data_unreadable", |w| {
        chmod(w, "data/Mineshaft.dat", 0o200)
    }),
    ("saved_data_unwritable", |w| {
        chmod(w, "data/Mineshaft.dat", 0o400)
    }),
    ("region_unreadable", |w| {
        chmod(w, "region/r.-1.0.mca", 0o200)
    }),
    ("region_unwritable", |w| {
        chmod(w, "region/r.-1.0.mca", 0o400)
    }),
    ("data_folder_unlistable", |w| chmod(w, "data", 0o300)),
    ("data_folder_unwritable", |w| chmod(w, "data", 0o500)),
    ("saved_data_folder_is_a_file", |w| {
        fs::write(w.join("data/minecraft"), b"x").expect("file")
    }),
    ("region_copy_fails", |_| {}),
    ("region_folder_unlistable", |w| chmod(w, "region", 0o300)),
    ("dimension_folder_unlistable", |w| chmod(w, "DIM-1", 0o300)),
    ("journal_unreadable", |w| journal(w, b"{}", 0o200)),
    ("journal_invalid_utf8", |w| {
        journal(w, &[b'{', 0xff, b'}'], 0o644)
    }),
    ("journal_utf8_three_byte_lead", |w| {
        journal(w, b"{\xe1\x80A}", 0o644)
    }),
    ("journal_utf8_three_byte_lead_cut", |w| {
        journal(w, b"{\xe1\x80", 0o644)
    }),
    ("journal_utf8_four_byte_lead", |w| {
        journal(w, b"{\xf0\x9f\x98}", 0o644)
    }),
    ("journal_utf8_overlong_four_byte", |w| {
        journal(w, b"{\xf0\x80\x80\x80}", 0o644)
    }),
    ("journal_utf8_surrogate", |w| {
        journal(w, b"{\xed\xa0\x80}", 0o644)
    }),
    ("journal_empty", |w| journal(w, b"", 0o644)),
    ("journal_blank", |w| journal(w, b" \r\n\t", 0o644)),
    ("journal_bom_only", |w| journal(w, b"\xef\xbb\xbf", 0o644)),
    ("journal_comment_only", |w| journal(w, b"/* none */", 0o644)),
    ("journal_not_an_object", |w| journal(w, b"[]", 0o644)),
    ("journal_trailing_data", |w| {
        journal(w, br#"{"moves":[]} x"#, 0o644)
    }),
    ("journal_cut", |w| journal(w, br#"{"moves":"#, 0o644)),
    ("journal_without_moves", |w| journal(w, b"{}", 0o644)),
    ("journal_move_without_target", |w| {
        journal(w, br#"{"moves":[{"from":"a"}]}"#, 0o644)
    }),
    ("journal_moves_not_a_list", |w| {
        journal(w, br#"{"moves":{}}"#, 0o644)
    }),
    ("journal_move_not_an_object", |w| {
        journal(w, br#"{"moves":[5]}"#, 0o644)
    }),
    ("journal_move_from_number", |w| {
        journal(w, br#"{"moves":[{"from":5,"to":"b"}]}"#, 0o644)
    }),
    ("journal_moves_null", |w| {
        journal(w, br#"{"moves":null}"#, 0o644)
    }),
    ("journal_move_without_paths", |w| {
        journal(w, br#"{"moves":[{}]}"#, 0o644)
    }),
    ("journal_move_null_source", |w| {
        journal(w, br#"{"moves":[{"from":null,"to":"b"}]}"#, 0o644)
    }),
    ("journal_move_both_outside", |w| {
        journal(w, br#"{"moves":[{"from":"../a","to":"../b"}]}"#, 0o644)
    }),
    ("journal_two_moves_not_objects", |w| {
        journal(w, br#"{"moves":[5,6,7]}"#, 0o644)
    }),
    ("journal_move_to_empty_path", |w| {
        journal(w, br#"{"moves":[{"from":"a/..","to":"b"}]}"#, 0o644)
    }),
    ("journal_move_outside", |w| {
        journal(w, br#"{"moves":[{"from":"../a","to":"b"}]}"#, 0o644)
    }),
];

/// The cases that cap the size of the files the server writes: a copy of the 561 KB and 3 MB region files into the
/// temporary directory fails.
fn file_size_limit(case: &str) -> Option<u64> {
    (case == "region_copy_fails").then_some(256 * 1024)
}

const READ_FAILED: &str = "WARN Failed to read chunk [";

/// The `Failed to read chunk` records `mc-world` logs for a case: it always logs all of them.
fn ours_reads(case: &str) -> usize {
    match case {
        "region_unreadable" | "region_copy_fails" => 2,
        _ => 0,
    }
}

/// The records after the first `Failed to read chunk` of a folder that cannot be read: a worker of the I/O pool of
/// vanilla logs them while the main thread already fails the upgrade and stops the server, so a run of the real server
/// may end without them (12 of 13 runs on the unreadable region logged the second, one did not; the first record always
/// came out). The comparison drops them from ours when the real log has fewer records than ours.
fn without_late_reads(log: Vec<String>) -> Vec<String> {
    let mut reads = 0;
    let mut skipping = false;
    log.into_iter()
        .filter(|line| {
            if line.starts_with("  ") {
                return !skipping;
            }
            if line.starts_with(READ_FAILED) {
                reads += 1;
                skipping = reads > 1;
            } else {
                skipping = false;
            }
            !skipping
        })
        .collect()
}

fn upgrade(side: Side, dir: &Path, case: &str, damage: Damage) -> Run {
    let world = old_world();
    run_limited(
        side,
        dir,
        move |d| {
            copy_dir(&world, &d.join("world"));
            let _ = fs::remove_file(d.join("world/session.lock"));
            damage(&d.join("world"));
        },
        &[],
        file_size_limit(case),
    )
}

#[test]
#[ignore = "oracle: spawns vanilla server"]
fn unreadable_world_files() {
    let only = std::env::var("MC_CASE").ok();
    let mut failures = Vec::new();
    for (name, damage) in CASES {
        if only.as_deref().is_some_and(|o| o != name) {
            continue;
        }
        let dirs = [
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        ];
        let runs: Vec<Run> = [Side::Vanilla, Side::Ours]
            .into_iter()
            .zip(&dirs)
            .map(|(side, dir)| upgrade(side, dir.path(), name, damage))
            .collect();
        for dir in &dirs {
            restore(dir.path());
        }
        let mut findings = Vec::new();
        if (runs[0].started, runs[0].exit) != (runs[1].started, runs[1].exit) {
            findings.push(format!(
                "outcome: vanilla started={} exit={:?}, ours started={} exit={:?}",
                runs[0].started, runs[0].exit, runs[1].started, runs[1].exit
            ));
        }
        let (vanilla, mut ours) = (gate_log(&runs[0]), gate_log(&runs[1]));
        let reads = |log: &[String]| log.iter().filter(|l| l.starts_with(READ_FAILED)).count();
        // ours is deterministic: its count of records is exact, and only the real log may lack the late ones
        if reads(&ours) != ours_reads(name) {
            findings.push(format!(
                "ours logged {} `Failed to read chunk` records, expected {}",
                reads(&ours),
                ours_reads(name)
            ));
        }
        if reads(&vanilla) < reads(&ours) {
            ours = without_late_reads(ours);
        }
        println!("UNREADABLE {name}: vanilla log: {vanilla:#?}");
        if let Some(d) = first_log_difference(&vanilla, &ours) {
            findings.push(d);
        }
        println!(
            "UNREADABLE {name}: {}",
            if findings.is_empty() {
                "equal"
            } else {
                "differs"
            }
        );
        if !findings.is_empty() {
            failures.push(format!("== {name}\n{}", findings.join("\n")));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
