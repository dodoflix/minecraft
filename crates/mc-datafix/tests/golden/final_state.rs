//! The data fixers are complete: the chain has no empty version range, every `DataFixTypes` constant that a server call
//! site uses is wired into the chain, the start-up has no minimum data version, and no text claims that data fixing is
//! missing in the code that runs it.

use mc_datafix::fixer::get_version;
use mc_datafix::{CURRENT_VERSION, DataFixTypes, data_fixer};
use mc_nbt::CompoundTag;
use std::collections::BTreeSet;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const EXPECTED_MISSING_FIXES: &str = include_str!("../fixtures/chain_expected_missing.tsv");
const EXPECTED_MISSING_SCHEMAS: &str =
    include_str!("../fixtures/schema_types_expected_missing.txt");

fn crates() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs")
            && path.file_name().is_some_and(|n| n != "final_state.rs")
        {
            out.push(path);
        }
    }
}

fn files_under(dirs: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in dirs {
        rust_files(&crates().join(dir), &mut out);
    }
    assert!(!out.is_empty(), "{dirs:?}: no source files found");
    out
}

#[test]
fn every_version_range_registers_fixes_and_the_ranges_reach_the_current_version() {
    let mut ranges: Vec<(i32, i32)> = fs::read_dir(crates().join("mc-datafix/src/fixers"))
        .expect("fixers directory")
        .filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            let (from, to) = name
                .strip_prefix("range_")?
                .strip_suffix(".rs")?
                .split_once('_')?;
            Some((from.parse().ok()?, to.parse().ok()?))
        })
        .collect();
    ranges.sort();
    assert_eq!(
        ranges.len(),
        9,
        "the data fixer registers nine version ranges"
    );
    assert_eq!(ranges.last().map(|r| r.1), Some(CURRENT_VERSION));
    for pair in ranges.windows(2) {
        assert_eq!(
            pair[0].1 + 1,
            pair[1].0,
            "the ranges are contiguous: {pair:?}"
        );
    }
    let versions: BTreeSet<i32> = data_fixer()
        .fixes()
        .iter()
        .map(|fix| get_version(fix.version_key()))
        .collect();
    for (from, to) in ranges {
        assert!(
            versions.range(from..=to).next().is_some(),
            "no fix registered in the range {from}..={to}"
        );
    }
    assert!(
        EXPECTED_MISSING_FIXES.trim().is_empty(),
        "a fix of the chain is not registered"
    );
    assert!(
        EXPECTED_MISSING_SCHEMAS.trim().is_empty(),
        "a schema type is not registered"
    );
}

/// The `DataFixTypes` constants named by the code the server runs (the part of a file before its test module).
fn constants_with_a_server_call_site() -> BTreeSet<&'static str> {
    let names: Vec<&'static str> = DataFixTypes::ALL.iter().map(DataFixTypes::name).collect();
    let mut used = BTreeSet::new();
    for dir in fs::read_dir(crates()).expect("crates directory") {
        let dir = dir.expect("directory entry").path();
        if dir.ends_with("mc-datafix") || !dir.join("src").is_dir() {
            continue;
        }
        let mut files = Vec::new();
        rust_files(&dir.join("src"), &mut files);
        for file in files {
            if file.file_name().is_some_and(|n| n == "tests.rs") {
                continue;
            }
            let text = fs::read_to_string(&file).expect("source file");
            let code = text.split("#[cfg(test)]").next().unwrap_or_default();
            for name in &names {
                if code.contains(&format!("DataFixTypes::{name}")) {
                    used.insert(*name);
                }
            }
        }
    }
    used
}

/// The lowest schema version that has the type of `ty`.
fn first_version_with(ty: &DataFixTypes) -> Option<i32> {
    let reference = ty.type_ref();
    data_fixer()
        .schema_keys()
        .find(|key| {
            catch_unwind(AssertUnwindSafe(|| {
                data_fixer().get_schema(*key).get_type_raw(&reference)
            }))
            .is_ok()
        })
        .map(get_version)
}

#[test]
fn every_data_fix_type_the_server_uses_is_wired_into_the_chain() {
    let used = constants_with_a_server_call_site();
    assert!(
        used.len() >= 25,
        "the server call sites were not found: {used:?}"
    );
    for ty in DataFixTypes::ALL.iter().filter(|t| used.contains(t.name())) {
        // a type exists from some schema on (`advancements` is newer than the chain's start); `update` resolves it at
        // both ends of the range and panics when the chain lacks it
        let from =
            first_version_with(ty).unwrap_or_else(|| panic!("{} is in no schema", ty.name()));
        ty.update_tag_to_current_version(data_fixer(), &CompoundTag::new(), from);
    }
}

#[test]
fn the_launch_has_no_minimum_data_version() {
    for file in files_under(&["mc-game-core/src/server/dedicated", "mc-world/src"]) {
        let text = fs::read_to_string(&file).expect("source file");
        assert!(
            !text.contains("OLDEST_SUPPORTED_DATA_VERSION"),
            "{} keeps a minimum data version",
            file.display()
        );
    }
}

#[test]
fn no_text_claims_that_data_fixing_is_missing() {
    const CLAIMS: [&str; 6] = [
        "no dfu port",
        "not ported",
        "data fixing is not supported",
        "file fixer is not available",
        "upgrader is not available",
        "no concrete fix",
    ];
    let files = files_under(&[
        "mc-datafix",
        "mc-world/src/filefix",
        "mc-world/src/level_storage",
        "mc-world/src/region",
        "mc-game-core/src/server/dedicated",
        "mc-game-core/src/server/world_upgrader",
    ]);
    let mut found = Vec::new();
    for file in files {
        let text = fs::read_to_string(&file).expect("source file");
        for (i, line) in text.lines().enumerate() {
            let lower = line.to_lowercase();
            if CLAIMS.iter().any(|c| lower.contains(c)) {
                found.push(format!("{}:{}: {}", file.display(), i + 1, line.trim()));
            }
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}
