//! The file fixer on whole worlds against vanilla's `FileFixerUpper`: the log lines, the level data it returns, the
//! tree of the saves folder afterwards and the summary of the fixed world, recorded by
//! `scripts/datafix-oracle.sh filefix` (record format: `crates/mc-datafix/tests/oracle/FileFixVerbs.java`) for the
//! minimal worlds of `fixtures/filefix/worlds` and, when the scripted worlds of old servers are in the cache, for each
//! of those (`fixtures/filefix/corpus`). `1.16.5-conflict` is a world the fix cannot upgrade: the record holds the text of
//! the exception vanilla throws.

use std::cell::RefCell;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Once;

use mc_core::pos::chunk_pos::ChunkPos;
use mc_nbt::{CompoundTag, NbtAccounter, Tag};
use mc_world::filefix::{
    Checkpoint, FileFixError, FileFixerUpper, FileSystemCapabilities, FixHooks, UpgradeProgress,
    file_fixer, new_file_fixer,
};
use mc_world::level_storage::{LevelStorageSource, LevelSummary};
use mc_world::region::RegionFile;

use crate::region_common::{info, sha256_hex};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/filefix")
}

fn corpus_worlds() -> PathBuf {
    std::env::var_os("MC_DATAFIX_CACHE")
        .map(PathBuf::from)
        .or_else(|| {
            let cache = std::env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".cache")))?;
            Some(cache.join("mc-datafix-corpus"))
        })
        .unwrap_or_default()
        .join("worlds")
}

// ---- log capture ---------------------------------------------------------------------------------------------

thread_local! {
    static LOGGED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

struct CaptureLogger;

impl log::Log for CaptureLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }

    /// The events of the file fixer (`net.minecraft.util.filefix` in vanilla) and every error.
    fn log(&self, record: &log::Record) {
        let level = match record.level() {
            log::Level::Error => "ERROR",
            log::Level::Warn => "WARN",
            log::Level::Info => "INFO",
            log::Level::Debug => "DEBUG",
            log::Level::Trace => return,
        };
        if record.level() == log::Level::Error || record.target().starts_with("mc_world::filefix") {
            LOGGED.with(|l| l.borrow_mut().push(format!("{level}: {}", record.args())));
        }
    }

    fn flush(&self) {}
}

fn capture_logs<R>(f: impl FnOnce() -> R) -> (R, Vec<String>) {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        log::set_logger(&CaptureLogger).expect("no other logger in this test binary");
        log::set_max_level(log::LevelFilter::Debug);
    });
    LOGGED.with(|l| l.borrow_mut().clear());
    let result = f();
    (
        result,
        LOGGED.with(|l| std::mem::take(&mut *l.borrow_mut())),
    )
}

// ---- the record ----------------------------------------------------------------------------------------------

/// `DataFixerOracle.escape`.
fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        // the repository tracks no zip files: a `*.zip.fixture` is the zip of the world
        let name = name.to_str().unwrap();
        let target = to.join(name.strip_suffix(".fixture").unwrap_or(name));
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn nbt_hash(tag: &CompoundTag) -> String {
    sha256_hex(&mc_nbt::nbt_io::to_bytes(tag))
}

fn regions(path: &Path, rel: &str, lines: &mut Vec<String>) {
    let name = path.file_name().unwrap().to_str().unwrap();
    let parts: Vec<&str> = name.split('.').collect();
    let (region_x, region_z): (i32, i32) = (parts[1].parse().unwrap(), parts[2].parse().unwrap());
    let file = RegionFile::new(info(), path, path.parent().unwrap(), false).unwrap();
    let mut all = String::new();
    let mut count = 0;
    for z in 0..32 {
        for x in 0..32 {
            let pos = ChunkPos::new(region_x * 32 + x, region_z * 32 + z);
            if !file.has_chunk(&pos) {
                continue;
            }
            let Some(mut stream) = file.get_chunk_data_input_stream(&pos).unwrap() else {
                continue;
            };
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes).unwrap();
            let tag = mc_nbt::nbt_io::read(&mut &bytes[..], &mut NbtAccounter::unlimited_heap())
                .unwrap_or_else(|e| panic!("{rel} chunk {} {}: {e}", pos.x, pos.z));
            count += 1;
            all.push_str(&format!("{},{},{}\n", pos.x, pos.z, nbt_hash(&tag)));
        }
    }
    lines.push(format!("R {rel} {count} {}", sha256_hex(all.as_bytes())));
}

fn file_line(path: &Path, rel: &str, lines: &mut Vec<String>) {
    let name = path.file_name().unwrap().to_str().unwrap();
    if name.ends_with(".mca") {
        regions(path, rel, lines);
        return;
    }
    let bytes = fs::read(path).unwrap();
    if name == "level.dat" {
        let mut root =
            mc_nbt::nbt_io::read_compressed_file(path, &mut NbtAccounter::unlimited_heap())
                .unwrap();
        if let Some(Tag::Compound(data)) = root.get_mut("Data") {
            data.put_long("LastPlayed", 0);
        }
        let raw = mc_nbt::nbt_io::to_bytes(&root);
        lines.push(format!("F {rel} {} {}", raw.len(), sha256_hex(&raw)));
    } else if name.ends_with(".dat") || name.ends_with(".dat_old") {
        let mut raw = Vec::new();
        match flate2::read::GzDecoder::new(&bytes[..]).read_to_end(&mut raw) {
            Ok(_) => lines.push(format!("F {rel} {} {}", raw.len(), sha256_hex(&raw))),
            Err(_) => lines.push(format!(
                "F {rel} {} {} raw",
                bytes.len(),
                sha256_hex(&bytes)
            )),
        }
    } else {
        lines.push(format!("F {rel} {} {}", bytes.len(), sha256_hex(&bytes)));
    }
}

fn list_tree(root: &Path, dir: &Path, lines: &mut Vec<String>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    for path in entries {
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        if fs::symlink_metadata(&path).unwrap().is_dir() {
            lines.push(format!("D {rel}"));
            list_tree(root, &path, lines);
        } else {
            file_line(&path, &rel, lines);
        }
    }
}

fn summary_line(s: &LevelSummary) -> String {
    let settings = s.settings().expect("a normal summary");
    let version = s.level_version().expect("a normal summary");
    let mode = match settings.game_type {
        mc_world::level_storage::GameType::Survival => "SURVIVAL",
        mc_world::level_storage::GameType::Creative => "CREATIVE",
        mc_world::level_storage::GameType::Adventure => "ADVENTURE",
        mc_world::level_storage::GameType::Spectator => "SPECTATOR",
    };
    format!(
        "name={} mode={mode} hardcore={} commands={} experimental={} manual={} fileFix={} compatible={} backup={} shouldBackup={} downgrade={} disabled={} upload={} edit={} recreate={} version={},{},{},{}",
        s.level_name(),
        settings.difficulty_settings.hardcore,
        settings.allow_commands,
        s.experimental,
        s.requires_manual_conversion,
        s.requires_file_fixing,
        s.is_compatible(),
        s.backup_status().translation_key(),
        s.should_backup(),
        s.is_downgrade(),
        s.is_disabled(),
        s.can_upload(),
        s.can_edit(),
        s.can_recreate(),
        version.minecraft_version_name,
        version.minecraft_version.version,
        version.minecraft_version.series,
        version.snapshot
    )
}

/// The record of `FileFixVerbs.java` for the world folder `world` (`saves`: the saves folder holding it), made with the
/// file fixer of this crate.
fn record(world: &Path, saves: bool) -> Vec<String> {
    let dir = tempfile::tempdir().unwrap();
    copy_tree(
        world,
        &if saves {
            dir.path().to_owned()
        } else {
            dir.path().join("world")
        },
    );
    record_in(dir.path(), file_fixer())
}

/// `FileFixerUpper.fix` on the level `world` of `source`.
fn fix_world(
    source: &LevelStorageSource,
    fixer: &FileFixerUpper,
) -> Result<CompoundTag, FileFixError> {
    let mut access = source.validate_and_create_access("world").unwrap();
    let unfixed = access.get_unfixed_data_tag_with_fallback().unwrap();
    fixer.fix(&mut access, unfixed, &UpgradeProgress::noop())
}

/// The record for the level `world` of the saves folder `saves_dir`.
fn record_in(saves_dir: &Path, fixer: &FileFixerUpper) -> Vec<String> {
    let saves = saves_dir;
    let source = LevelStorageSource::create_default(saves).unwrap();
    let mut lines = Vec::new();
    let (fixed, logged) = capture_logs(|| fix_world(&source, fixer));
    let shown = saves.to_str().unwrap();
    lines.extend(
        logged
            .iter()
            .map(|l| format!("LOG\t{}", escape(&l.replace(shown, "<saves>")))),
    );
    match fixed {
        Ok(mut tag) => {
            tag.put_long("LastPlayed", 0);
            lines.push(format!(
                "FIXED\t{}\t{}",
                nbt_hash(&tag),
                escape(&Tag::Compound(tag).to_string())
            ));
        }
        Err(e) => lines.push(format!(
            "THREW\t{}",
            escape(&e.to_string().replace(shown, "<saves>"))
        )),
    }
    let mut tree = Vec::new();
    list_tree(saves, saves, &mut tree);
    lines.extend(
        tree.iter()
            .map(|l| format!("TREE\t{}", escape(&l.replace(shown, "<saves>")))),
    );
    let access = source.validate_and_create_access("world").unwrap();
    let summary = access
        .get_unfixed_data_tag_with_fallback()
        .and_then(|tag| access.fix_and_get_summary_from_tag(&tag))
        .map(|s| summary_line(&s))
        .unwrap_or_else(|e| format!("ERROR {e}"));
    lines.push(format!("SUMMARY\t{summary}"));
    lines
}

/// The first lines where `ours` and the recording differ; the `FIXED` text form is only for the reader, the hash decides.
fn differences(id: &str, ours: &[String], recorded: &[String]) -> Option<String> {
    let key = |l: &String| {
        if l.starts_with("FIXED\t") {
            l.split('\t').take(2).collect::<Vec<_>>().join("\t")
        } else {
            l.clone()
        }
    };
    let (a, b): (Vec<String>, Vec<String>) = (
        ours.iter().map(key).collect(),
        recorded.iter().map(key).collect(),
    );
    if a == b {
        return None;
    }
    let mut out = format!(
        "{id}: the record differs from vanilla's ({} vs {} lines)\n",
        a.len(),
        b.len()
    );
    let mut shown = 0;
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i), b.get(i));
        if x != y {
            out.push_str(&format!(
                "line {}:\n  ours:    {}\n  vanilla: {}\n",
                i + 1,
                x.map_or("-", |s| s.as_str()),
                y.map_or("-", |s| s.as_str())
            ));
            shown += 1;
            if shown == 6 {
                break;
            }
        }
    }
    Some(out)
}

fn recorded_ids(folder: &Path) -> Vec<String> {
    let mut ids: Vec<String> = fs::read_dir(folder)
        .map(|entries| {
            entries
                .filter_map(|e| {
                    let name = e.ok()?.file_name().into_string().ok()?;
                    name.strip_suffix(".txt").map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    ids
}

/// Runs each recording of `records` on the world of the same name in `worlds` (`saves`: each is a saves folder); returns
/// what differs, how many were compared and the ids of the worlds that are not there.
fn replay(records: &Path, worlds: &Path, saves: bool) -> (Vec<String>, usize, Vec<String>) {
    let (mut failures, mut compared, mut missing) = (Vec::new(), 0, Vec::new());
    for id in recorded_ids(records) {
        let world = worlds.join(&id);
        if !world.is_dir() {
            missing.push(id);
            continue;
        }
        let recorded: Vec<String> = fs::read_to_string(records.join(format!("{id}.txt")))
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect();
        compared += 1;
        if let Some(diff) = differences(&id, &record(&world, saves), &recorded) {
            failures.push(diff);
        }
    }
    (failures, compared, missing)
}

#[test]
fn the_minimal_worlds_are_fixed_like_vanilla_does() {
    let (failures, compared, missing) = replay(&fixtures(), &fixtures().join("worlds"), false);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(missing.is_empty(), "no world for the records {missing:?}");
    assert!(compared >= 10, "{compared} worlds compared");
}

#[test]
#[allow(
    clippy::print_stderr,
    reason = "says which scripted worlds this machine does not have"
)]
fn the_scripted_worlds_of_old_servers_are_fixed_like_vanilla_does() {
    let worlds = corpus_worlds();
    let (failures, compared, missing) = replay(&fixtures().join("corpus"), &worlds, false);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    if !missing.is_empty() {
        eprintln!(
            "not compared, not in {} (scripts/datafix-worlds.sh makes them): {missing:?}; {compared} compared",
            worlds.display()
        );
    }
}

#[test]
fn what_an_earlier_start_left_next_to_the_world_is_dealt_with_like_vanilla_does() {
    let interrupted = fixtures().join("interrupted");
    let (failures, compared, missing) = replay(&interrupted, &interrupted, true);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        missing.is_empty(),
        "no saves folder for the records {missing:?}"
    );
    assert_eq!(compared, 2);
}

// ---- an upgrade that was killed ------------------------------------------------------------------------------------

/// A file system without hard links, where the moves are journaled, that stops the upgrade at `abort_at`.
struct Journaled {
    abort_at: Option<Checkpoint>,
}

impl FixHooks for Journaled {
    fn file_system_capabilities(&self, _: &Path) -> std::io::Result<FileSystemCapabilities> {
        Ok(FileSystemCapabilities {
            atomic_move: true,
            hard_links: false,
        })
    }

    fn checkpoint(&self, at: Checkpoint) -> std::io::Result<()> {
        if self.abort_at == Some(at) {
            std::process::abort();
        }
        Ok(())
    }
}

const CHILD_ENV: &str = "MC_FILE_FIX_KILLED_AT";

/// The body of the process that [`killed_at`] starts: it upgrades the world and aborts at the checkpoint.
#[test]
fn child_process_entry() {
    let Ok(spec) = std::env::var(CHILD_ENV) else {
        return;
    };
    let (saves, checkpoint) = spec.split_once('|').unwrap();
    let abort_at = match checkpoint {
        "FixesApplied" => Checkpoint::FixesApplied,
        "JournalWritten" => Checkpoint::JournalWritten,
        "NewWorldAtTopLevel" => Checkpoint::NewWorldAtTopLevel,
        other => Checkpoint::MoveFile(other.strip_prefix("MoveFile").unwrap().parse().unwrap()),
    };
    let fixer = new_file_fixer().with_hooks(Box::new(Journaled {
        abort_at: Some(abort_at),
    }));
    let source = LevelStorageSource::create_default(saves).unwrap();
    let _ = fix_world(&source, &fixer);
}

fn killed_at(saves: &Path, checkpoint: &str) {
    use std::os::unix::process::ExitStatusExt;
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "file_fix::child_process_entry",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ENV, format!("{}|{checkpoint}", saves.display()))
        .status()
        .unwrap();
    assert_eq!(status.signal(), Some(6), "the child aborts at {checkpoint}");
}

/// The lines that say what became of the files: the tree, the level data and the summary (the log of a continued
/// upgrade is another one).
fn outcome(lines: &[String]) -> Vec<&String> {
    lines.iter().filter(|l| !l.starts_with("LOG\t")).collect()
}

#[test]
fn an_upgrade_killed_on_a_file_system_without_hard_links_is_continued_to_the_world_vanilla_makes() {
    let recorded: Vec<String> = fs::read_to_string(fixtures().join("1.20.4.txt"))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    let recorded_outcome = outcome(&recorded);
    for checkpoint in [
        "FixesApplied",
        "JournalWritten",
        "MoveFile1",
        "MoveFile3",
        "NewWorldAtTopLevel",
    ] {
        let saves = tempfile::tempdir().unwrap();
        copy_tree(
            &fixtures().join("worlds/1.20.4"),
            &saves.path().join("world"),
        );
        killed_at(saves.path(), checkpoint);
        assert!(
            saves.path().join("world/filefix").is_dir(),
            "{checkpoint}: the killed upgrade leaves its staging folder"
        );
        let continued = new_file_fixer().with_hooks(Box::new(Journaled { abort_at: None }));
        let ours = record_in(saves.path(), &continued);
        let ours_outcome = outcome(&ours);
        let key = |l: &&String| {
            if l.starts_with("FIXED\t") {
                l.split('\t').take(2).collect::<Vec<_>>().join("\t")
            } else {
                (*l).clone()
            }
        };
        assert_eq!(
            ours_outcome.iter().map(key).collect::<Vec<_>>(),
            recorded_outcome.iter().map(key).collect::<Vec<_>>(),
            "{checkpoint}: the continued upgrade ends in another world than vanilla's"
        );
    }
}
