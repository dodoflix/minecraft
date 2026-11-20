//! Legacy user list conversion: the real server and `mc-server` start in offline mode (`online-mode=false`, so the
//! conversion makes offline UUIDs and asks no profile service) on identical directories with the old files
//! `ops.txt`, `white-list.txt`, `banned-players.txt`, `banned-ips.txt` and `world/players/<name>.dat`, then stop. What
//! they leave is compared exactly: the new lists, the renamed old files (`*.converted`), the moved player files, the
//! user cache (expiry times left out: wall clock) and the warnings and errors of the log.
//!
//! The online-mode variant needs a profile service both servers can be pointed at; ours has no such hook yet.
use std::fs;
use std::path::Path;

use mc_nbt::CompoundTag;
use mc_nbt::compress::write_compressed;
use worldcmp::fields::{FieldSpec, Ignores};
use worldcmp::tree::{self, TreeOptions, compare_trees};

use crate::world_start::{Run, Side, first_log_difference, run};

const OPS: &str = "\
Steve
alex_1
# not a comment: every line of the old op list is a name

Notch
";

const WHITE_LIST: &str = "Steve\nHerobrine\nalex_1\n";

/// `name|created|source|expires|reason`; the old format lower-cases the key, so `Alex_B` converts as `alex_b`.
const BANNED_PLAYERS: &str = "\
# banned players
Alex_B|2013-01-01 00:00:00 +0000|Server|forever|Griefing|more|columns
Steve|2014-02-03 04:05:06 +0000|Console
Notch|2015-06-07 08:09:10 +0000|Console|2099-01-01 00:00:00 +0000|Time out
";

const BANNED_IPS: &str = "\
10.0.0.1|2013-01-01 00:00:00 +0000|Server|forever|Spam
192.168.1.20|2016-01-02 03:04:05 +0000|Console|2099-01-01 00:00:00 +0000|Flood
";

fn player_file(dir: &Path, name: &str) {
    let mut tag = CompoundTag::new();
    tag.put_string("name", name);
    tag.put_int("DataVersion", 5023);
    fs::write(dir.join(name), write_compressed(&tag)).expect("player file");
}

/// The legacy files, with player files named `players`.
fn prepare(dir: &Path, players: &[&str]) {
    for (name, text) in [
        ("ops.txt", OPS),
        ("white-list.txt", WHITE_LIST),
        ("banned-players.txt", BANNED_PLAYERS),
        ("banned-ips.txt", BANNED_IPS),
    ] {
        fs::write(dir.join(name), text).expect("legacy list");
    }
    let old = dir.join("world/players");
    fs::create_dir_all(&old).expect("players folder");
    for name in players {
        player_file(&old, name);
    }
}

/// Copies what a conversion changes into a directory of its own: the lists, their renamed originals, the user cache and the player folders.
fn view(run: &Run) -> std::path::PathBuf {
    let view = run.dir.join("view");
    fs::create_dir_all(&view).expect("view");
    for entry in fs::read_dir(&run.dir).expect("server dir").flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let list =
            name.ends_with(".json") || name.ends_with(".converted") || name.ends_with(".txt");
        if list && name != "eula.txt" {
            fs::copy(entry.path(), view.join(&name)).expect("copy list");
        }
    }
    for folder in ["world/players", "unknownplayers", "world/unknownplayers"] {
        let from = run.dir.join(folder);
        if from.is_dir() {
            let to = view.join(folder);
            fs::create_dir_all(to.parent().expect("parent")).expect("view folder");
            let status = std::process::Command::new("cp")
                .arg("-r")
                .arg(&from)
                .arg(&to)
                .status()
                .expect("cp");
            assert!(status.success());
        }
    }
    view
}

fn compare(vanilla: &Run, ours: &Run) -> Vec<String> {
    let mut findings = Vec::new();
    if (vanilla.started, vanilla.exit) != (ours.started, ours.exit) {
        findings.push(format!(
            "outcome: vanilla started={} exit={:?}, ours started={} exit={:?}",
            vanilla.started, vanilla.exit, ours.started, ours.exit
        ));
    }
    if let Some(d) = first_log_difference(&vanilla.log, &ours.log) {
        findings.push(d);
    }
    let ignores = Ignores(
        // the cache entries expire a month after they were made
        ["usercache.json#*.expiresOn"]
            .iter()
            .map(|s| s.parse::<FieldSpec>().expect("spec"))
            .collect(),
    );
    let files = compare_trees(
        &view(vanilla),
        &view(ours),
        &TreeOptions {
            ignores,
            chunks: None,
            max_listed: 10,
        },
    );
    if !files.is_clean() {
        findings.push(tree::render_text("vanilla", "ours", &files, 20));
    }
    findings
}

/// `fails`: whether the conversion of the player files is expected to fail (the lists are converted and the server starts anyway).
fn dual(players: &'static [&'static str], fails: bool) {
    let dirs = [
        tempfile::tempdir().expect("temp dir"),
        tempfile::tempdir().expect("temp dir"),
    ];
    let args: [&str; 0] = [];
    let vanilla = run(
        Side::Vanilla,
        dirs[0].path(),
        move |dir| prepare(dir, players),
        &args,
    );
    let ours = run(
        Side::Ours,
        dirs[1].path(),
        move |dir| prepare(dir, players),
        &args,
    );
    // the comparison must have seen a conversion, not two servers that stopped at the same point
    assert!(vanilla.started, "{:?}", vanilla.log);
    assert!(
        dirs[0].path().join("ops.txt.converted").exists(),
        "{:?}",
        vanilla.log
    );
    let failed = vanilla.log.iter().any(|l| l.contains("Conversion failed"));
    assert_eq!(failed, fails, "{:?}", vanilla.log);
    let findings = compare(&vanilla, &ours);
    assert!(findings.is_empty(), "{}", findings.join("\n"));
}

#[test]
#[ignore = "oracle: spawns vanilla server"]
fn offline_lists_and_player_files() {
    dual(
        &["Steve.dat", "alex_1.dat", "Notch.dat", "Mixed_Case.dat"],
        false,
    );
}

/// The player file is looked up as `<name>.dat`: with an upper-case extension the rename fails, the conversion of the player files is reported as failed and retried, and the server starts anyway.
#[test]
#[ignore = "oracle: spawns vanilla server"]
fn player_file_with_an_upper_case_extension() {
    dual(&["Steve.dat", "Herobrine.DAT"], true);
}
