//! A world whose file fix was interrupted: what an earlier start left in the world folder (`filefix/`, with or without
//! `upgrade_in_progress.json`, `filefix/new_world`) and next to it (`<world> upgraded`) is continued, or thrown away and
//! redone, by the real 26.3 server and by `mc-server` alike, and the two server directories are compared as in
//! [`crate::world_start`].
//!
//! The interrupted worlds are made by `mc-world`'s file fixer, which is stopped by a panic at a step of the upgrade (a
//! killed process leaves exactly what the steps before it did): with hard links (the file system's own capability) after the
//! fixes ran and after the new world was moved to the top level, and without hard links (the moves are journaled) after the
//! journal was written, in the middle of the moves and after the new world was moved to the top level. The journal of
//! `mc-world` has one key that vanilla's has not (`upgraded_folder`, which vanilla's reader ignores).
//!
//! The last journal state is the one `upgraded_folder` exists for: the files were moved and the new world already sits in
//! `<world> upgraded`, so the journal has nothing left to continue from. Vanilla fails on it (`Not a regular file`, it
//! looks for the moved files in the staging folder that is gone); `mc-server` finishes the swap. There the real server
//! upgrades the same world without an interruption, and the comparison skips the log (the interrupted start logs the
//! continuation).

use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use mc_world::filefix::{
    Checkpoint, FileSystemCapabilities, FixHooks, UpgradeProgress, new_file_fixer,
};
use mc_world::level_storage::LevelStorageSource;

use crate::world_start::{Side, compare, copy_dir, run, stop_random_ticks, worlds};

/// The file system of the interrupted upgrade, and where it stops.
struct Crash {
    at: Checkpoint,
    /// `None`: what the file system under the test offers.
    hard_links: Option<bool>,
}

impl FixHooks for Crash {
    fn file_system_capabilities(&self, dir: &Path) -> std::io::Result<FileSystemCapabilities> {
        let real = mc_world::filefix::detect_file_system_capabilities(dir)?;
        Ok(FileSystemCapabilities {
            hard_links: self.hard_links.unwrap_or(real.hard_links),
            ..real
        })
    }

    fn checkpoint(&self, at: Checkpoint) -> std::io::Result<()> {
        assert!(at != self.at, "interrupted at {at:?}");
        Ok(())
    }
}

/// Upgrades the world folder `saves/world` until `crash` stops it.
fn interrupt(saves: &Path, crash: Crash) {
    let source = LevelStorageSource::create_default(saves).expect("saves folder");
    let mut access = source
        .validate_and_create_access("world")
        .expect("world access");
    let unfixed = access
        .get_unfixed_data_tag_with_fallback()
        .expect("level.dat");
    let fixer = new_file_fixer().with_hooks(Box::new(crash));
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        fixer.fix(&mut access, unfixed, &UpgradeProgress::default())
    }));
    assert!(outcome.is_err(), "the upgrade was not interrupted");
}

/// The state in which vanilla cannot continue (see the module documentation).
const VANILLA_FAILS: &str = "journal_new_world_at_top_level";

const STATES: &[(&str, Checkpoint, Option<bool>)] = &[
    ("hard_links_fixes_applied", Checkpoint::FixesApplied, None),
    (
        "hard_links_new_world_at_top_level",
        Checkpoint::NewWorldAtTopLevel,
        None,
    ),
    ("journal_written", Checkpoint::JournalWritten, Some(false)),
    ("journal_mid_moves", Checkpoint::MoveFile(1), Some(false)),
    (
        "journal_new_world_at_top_level",
        Checkpoint::NewWorldAtTopLevel,
        Some(false),
    ),
];

#[test]
#[ignore = "oracle: spawns vanilla server"]
fn an_interrupted_file_fix_is_continued_like_vanilla_does() {
    let mut failures = Vec::new();
    for world in worlds() {
        for &(state, at, hard_links) in STATES {
            let staging = tempfile::tempdir().expect("temp dir");
            copy_dir(&world.dir, &staging.path().join("world"));
            stop_random_ticks(&staging.path().join("world"));
            interrupt(staging.path(), Crash { at, hard_links });
            let dirs = [
                tempfile::tempdir().expect("temp dir"),
                tempfile::tempdir().expect("temp dir"),
            ];
            let uninterrupted = tempfile::tempdir().expect("temp dir");
            copy_dir(&world.dir, &uninterrupted.path().join("world"));
            stop_random_ticks(&uninterrupted.path().join("world"));
            let mut runs = Vec::new();
            for (side, dir) in [Side::Vanilla, Side::Ours].into_iter().zip(&dirs) {
                let from = if state == VANILLA_FAILS && side == Side::Vanilla {
                    uninterrupted.path().to_owned()
                } else {
                    staging.path().to_owned()
                };
                runs.push(run(
                    side,
                    dir.path(),
                    move |dir| {
                        for entry in fs::read_dir(&from).expect("staging folder") {
                            let entry = entry.expect("entry");
                            copy_dir(&entry.path(), &dir.join(entry.file_name()));
                        }
                    },
                    &[],
                ));
            }
            if state == VANILLA_FAILS {
                runs.iter_mut().for_each(|run| run.log.clear());
            }
            let findings = compare(&runs[0], &runs[1]);
            println!(
                "FILE FIX RESUME {} {state}: {}",
                world.name,
                if findings.is_empty() {
                    "equal".to_owned()
                } else {
                    format!("{} finding(s)", findings.len())
                }
            );
            if !findings.is_empty() {
                failures.push(format!(
                    "== {} {state}\n{}",
                    world.name,
                    findings.join("\n")
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
