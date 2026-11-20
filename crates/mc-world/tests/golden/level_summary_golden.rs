//! The level data of a world as `LevelStorageAccess` reads it, against the real vanilla classes: the summary of
//! `fixAndGetSummaryFromTag`, the outcome of `collectIssues` and the level data `FileFixerUpper.fix` hands to the
//! world load, recorded by `fixtures/level_storage/LevelSummaryGolden.java` for level.dat files of every data version
//! from 4899 on and the shapes that end in an error.

use mc_nbt::{CompoundTag, Tag, snbt};
use mc_world::level_storage::{
    GameType, LevelStorageAccess, LevelStorageSource, LevelSummary, write_compressed,
};

const GOLDEN: &str = include_str!("../fixtures/level_storage/level_summary_golden.txt");

fn summary_line(s: &LevelSummary) -> String {
    let settings = s.settings().expect("a normal summary");
    let version = s.level_version().expect("a normal summary");
    let mode = match settings.game_type {
        GameType::Survival => "SURVIVAL",
        GameType::Creative => "CREATIVE",
        GameType::Adventure => "ADVENTURE",
        GameType::Spectator => "SPECTATOR",
    };
    format!(
        "name={} mode={mode} hardcore={} commands={} experimental={} lastPlayed={} manual={} fileFix={} compatible={} backup={} shouldBackup={} downgrade={} disabled={} upload={} edit={} recreate={} version={},{},{},{}",
        s.level_name(),
        settings.difficulty_settings.hardcore,
        settings.allow_commands,
        s.experimental,
        s.last_played(),
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

/// `NbtFormatException` is the only exception the summary throws.
fn failure(e: &std::io::Error) -> String {
    format!("net.minecraft.nbt.NbtFormatException: {e}")
}

fn open(data: &CompoundTag) -> (tempfile::TempDir, LevelStorageAccess) {
    let saves = tempfile::tempdir().unwrap();
    let world = saves.path().join("w");
    std::fs::create_dir_all(&world).unwrap();
    let mut root = CompoundTag::new();
    root.put("Data", Tag::Compound(data.clone()));
    write_compressed(&root, &world.join("level.dat")).unwrap();
    let access = LevelStorageSource::create_default(saves.path())
        .unwrap()
        .create_access("w")
        .unwrap();
    (saves, access)
}

#[test]
fn summary_issues_and_fixed_data_match_vanilla() {
    let mut cases = 0;
    let mut name = String::new();
    let mut input = None;
    let mut fixed_supported = 0;
    let oldest = mc_datafix::data_fixer().oldest_supported_version();
    // the summary of data older than the registered chain is made as read: its cases are compared once the chain covers them
    let supported = |data: &Option<CompoundTag>| {
        data.as_ref()
            .and_then(|d| d.get_int("DataVersion"))
            .is_some_and(|v| v >= oldest)
    };
    for line in GOLDEN.lines() {
        if let Some(n) = line.strip_prefix("@@ ") {
            name = n.to_owned();
        } else if let Some(i) = line.strip_prefix("IN ") {
            input =
                Some(snbt::parse_compound(i).unwrap_or_else(|e| panic!("{name}: {}", e.message())));
        } else if line.starts_with("SUMMARY ") && supported(&input) {
            let (_dir, access) = open(input.as_ref().unwrap());
            let unfixed = access.get_unfixed_data_tag_with_fallback().unwrap();
            let ours = match access.fix_and_get_summary_from_tag(&unfixed) {
                Ok(summary) => format!("SUMMARY {}", summary_line(&summary)),
                Err(e) => format!("SUMMARY ERROR {}", failure(&e)),
            };
            assert_eq!(ours, line, "{name}");
            cases += 1;
        } else if line.starts_with("ISSUES ") && supported(&input) {
            let (_dir, access) = open(input.as_ref().unwrap());
            let ours = match access.collect_issues(false) {
                Ok(()) => "ISSUES ok".to_owned(),
                Err(e) => format!("ISSUES ERROR {}", failure(&e)),
            };
            assert_eq!(ours, line, "{name}");
        } else if let Some(expected) = line.strip_prefix("FIXED ") {
            let data = input.as_ref().unwrap();
            if expected == "file fixing required" {
                continue;
            }
            let mut ours =
                LevelStorageSource::fix_level_data(data).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_ne!(ours.get("LastPlayed"), Some(&Tag::Long(0)), "{name}");
            ours.put_long("LastPlayed", 0);
            assert_eq!(ours, snbt::parse_compound(expected).unwrap(), "{name}");
            fixed_supported += 1;
        }
    }
    assert!(
        cases >= 135 && fixed_supported >= 135,
        "{cases} cases, {fixed_supported} fixed"
    );
}

/// Data older than the registered fix chain is summarised as read (the start-up refuses it right after) and cannot
/// be fixed; none of it panics.
#[test]
fn data_older_than_the_registered_chain_is_summarised_as_read_and_not_fixed() {
    let oldest = mc_datafix::data_fixer().oldest_supported_version();
    if oldest == i32::MIN {
        return;
    }
    let mut data = CompoundTag::new();
    data.put_int("DataVersion", oldest - 1);
    data.put_int("version", 19132);
    let (_dir, access) = open(&data);
    let summary = access.fix_and_get_summary_from_tag(&data).unwrap();
    assert!(summary.requires_manual_conversion);
    access.collect_issues(false).unwrap();
    assert_eq!(
        LevelStorageSource::fix_level_data(&data),
        Err(mc_datafix::UnsupportedVersion {
            from_version: oldest - 1,
            oldest_supported: oldest
        })
    );
}
