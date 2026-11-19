//! The facade of the global fixer: the version constants against the real vanilla data, and the entry point on a
//! fixer without anything to do.

use crate::recorded_chain::parse;
use mc_datafix::{
    CURRENT_VERSION, DataFixTypes, LATEST_FILE_FIXER_VERSION, requires_file_fixing,
    update_to_current, world_version_to_file_fixer_version,
};
use mc_nbt::CompoundTag;

const RECORDED: &str = include_str!("../fixtures/fixes.tsv");

#[test]
fn current_version_is_the_world_version_of_the_vanilla_jar() {
    let json = mc_data::datapack::read_string("version.json").expect("version.json is embedded");
    let json: serde_json::Value = serde_json::from_str(&json).expect("valid json");
    assert_eq!(json["world_version"], CURRENT_VERSION);
}

#[test]
fn latest_file_fixer_version_is_the_newest_recorded_file_fix() {
    let newest = parse(RECORDED)
        .iter()
        .filter(|r| r.kind == "file")
        .map(|r| r.version)
        .max();
    assert_eq!(newest, Some(LATEST_FILE_FIXER_VERSION));
}

#[test]
fn file_fixing_follows_the_file_fixer_version() {
    for (version, required) in [
        (-1, true),
        (4771, true),
        (LATEST_FILE_FIXER_VERSION - 1, true),
        (LATEST_FILE_FIXER_VERSION, false),
        (CURRENT_VERSION + 1, false),
    ] {
        assert_eq!(requires_file_fixing(version), required, "{version}");
    }
    assert_eq!(world_version_to_file_fixer_version(4771), 0);
    assert_eq!(world_version_to_file_fixer_version(4772), 4772);
}

#[test]
fn a_tag_of_the_current_or_a_newer_version_is_returned_unchanged() {
    let mut tag = CompoundTag::new();
    tag.put_int("DataVersion", CURRENT_VERSION);
    tag.put_string("note", "kept");
    for from in [CURRENT_VERSION, CURRENT_VERSION + 1] {
        for ty in DataFixTypes::ALL {
            assert_eq!(
                update_to_current(*ty, &tag, from),
                tag,
                "{ty:?} from {from}"
            );
        }
    }
}
