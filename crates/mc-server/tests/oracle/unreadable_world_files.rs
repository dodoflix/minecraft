//! An old world whose files or folders the server may not read, list or write: the real 26.3 server and `mc-server`
//! each upgrade a copy of it, and the warnings and errors of the two logs (with the first line of the exception behind
//! each) are compared in order. The file fixer reads, lists, copies and moves the real files, and the exception of a
//! failed operation names the path. `mc-world` stops at the first chunk of a folder it cannot read and logs one
//! `Failed to read chunk` line, where vanilla's asynchronous reads log a varying number of them, in the hash order of its
//! structure data: the comparison shows the failing reads once, without their position. Run with
//! `cargo test -p mc-server --test oracle unreadable_ -- --ignored --nocapture`.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::startup_gates::gate_log;
use crate::world_start::{Run, Side, cache_dir, copy_dir, first_log_difference, run};

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

const CASES: [(&str, Damage); 34] = [
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

/// The chunk reads that fail alike, shown once with their position left out: which chunk of the folder fails first
/// follows the hash order of the structure data in vanilla, and how many fail before the upgrade stops depends on how far
/// the asynchronous reads of vanilla run ahead of its first join.
fn collapse_chunk_reads(log: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in log {
        if let Some(rest) = line.strip_prefix("WARN Failed to read chunk [") {
            let tail = rest.split_once(']').map_or("", |(_, tail)| tail);
            out.push(format!("WARN Failed to read chunk <pos>{tail}"));
        } else if line.starts_with("  ")
            && out
                .last()
                .is_some_and(|last| last.starts_with("WARN Failed to read chunk <pos>"))
        {
            let last = out.last_mut().expect("checked");
            last.push('\n');
            last.push_str(&line);
        } else {
            out.push(line);
        }
    }
    out.dedup_by(|a, b| a == b && a.starts_with("WARN Failed to read chunk <pos>"));
    out
}

fn upgrade(side: Side, dir: &Path, damage: Damage) -> Run {
    let world = old_world();
    run(
        side,
        dir,
        move |d| {
            copy_dir(&world, &d.join("world"));
            let _ = fs::remove_file(d.join("world/session.lock"));
            damage(&d.join("world"));
        },
        &[],
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
            .map(|(side, dir)| upgrade(side, dir.path(), damage))
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
        let (vanilla, ours) = (
            collapse_chunk_reads(gate_log(&runs[0])),
            collapse_chunk_reads(gate_log(&runs[1])),
        );
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
