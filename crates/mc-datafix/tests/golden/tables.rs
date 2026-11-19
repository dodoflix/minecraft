//! The lookup tables of the fix classes (`src/fixes/data/*.tsv.gz`, written by `scripts/datafix-oracle.sh --regen` from
//! the real vanilla classes) and their accessors against the recorded results of the vanilla accessors
//! (`fixtures/table_accessors.tsv.gz`).

use mc_datafix::fixes::{block_state_data, tables};
use mc_nbt::{Tag, snbt};
use std::collections::BTreeSet;
use std::path::Path;

fn tag(text: &str) -> Tag {
    snbt::parse(text).unwrap_or_else(|e| panic!("{text}: {}", e.message()))
}

fn string(text: &str) -> String {
    tag(text).as_string().expect("a string").to_string()
}

/// The recorded rows of one accessor: the columns between the accessor name and the result, and the result.
fn rows(accessor: &str) -> Vec<(Vec<String>, String)> {
    let recording = mc_testkit::include_gz!("../fixtures/table_accessors.tsv.gz");
    let rows: Vec<(Vec<String>, String)> = recording
        .lines()
        .filter_map(|line| {
            let mut cols: Vec<String> = line.split('\t').map(str::to_string).collect();
            (cols[0] == accessor).then(|| {
                let result = cols.pop().expect("result column");
                (cols.split_off(1), result)
            })
        })
        .collect();
    assert!(!rows.is_empty(), "no recorded rows for {accessor}");
    rows
}

fn java_null_is_none(result: &str) -> Option<&str> {
    (result != "-").then_some(result)
}

#[test]
fn every_data_file_is_listed_and_parses() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixes/data");
    let files: BTreeSet<String> = std::fs::read_dir(&dir)
        .expect("src/fixes/data")
        .map(|e| {
            e.expect("entry")
                .file_name()
                .into_string()
                .expect("UTF-8 name")
                .strip_suffix(".tsv.gz")
                .expect("a .tsv.gz file")
                .to_string()
        })
        .collect();
    let listed: BTreeSet<String> = tables::names().map(str::to_string).collect();
    assert_eq!(files, listed, "tables::names against src/fixes/data");
    for name in tables::names() {
        let t = tables::table(name);
        assert!(
            t.class_name.starts_with("net.minecraft.util.datafix."),
            "{name}: {}",
            t.class_name
        );
        assert!(!t.fields.is_empty(), "{name} has no field");
        let text = mc_testkit::read_gz(dir.join(format!("{name}.tsv.gz")));
        let entries: usize = t
            .fields
            .iter()
            .map(|f| f.entries.len() + usize::from(f.default.is_some()))
            .sum();
        assert_eq!(
            entries,
            text.lines().filter(|l| !l.starts_with('#')).count(),
            "{name}: lines against parsed entries"
        );
    }
}

#[test]
fn block_state_data_dump_has_the_vanilla_shape() {
    let t = tables::table("block_state_data");
    assert_eq!(t.field("MAP").entries.len(), 4096);
    assert_eq!(t.field("BLOCK_DEFAULTS").entries.len(), 256);
    let property_maps = t
        .fields
        .iter()
        .filter(|f| {
            !["ID_BY_OLD", "ID_BY_OLD_NAME"].contains(&f.name.as_str()) && !f.entries.is_empty()
        })
        .filter(|f| f.kind == tables::Kind::Map)
        .count();
    assert_eq!(property_maps, 393, "the property map constants");
    for f in &t.fields {
        if f.kind == tables::Kind::Map && f.name != "ID_BY_OLD" && f.name != "ID_BY_OLD_NAME" {
            f.string_map(); // every property map is string to string
        }
    }
}

#[test]
fn block_state_data_accessors_equal_the_recording() {
    for (args, result) in rows("BlockStateData.getTag") {
        let id: i32 = args[0].parse().expect("id");
        assert_eq!(*block_state_data::get_tag(id), tag(&result), "getTag({id})");
    }
    for (args, result) in rows("BlockStateData.upgradeBlock(int)") {
        let id: i32 = args[0].parse().expect("id");
        assert_eq!(
            block_state_data::upgrade_block_id(id),
            result,
            "upgradeBlock({id})"
        );
    }
    for (args, result) in rows("BlockStateData.upgradeBlock(String)") {
        let name = string(&args[0]);
        assert_eq!(
            block_state_data::upgrade_block(&name),
            result,
            "upgradeBlock({name})"
        );
    }
    let cases = rows("BlockStateData.upgradeBlockStateTag");
    let upgraded = cases
        .iter()
        .filter(|(args, result)| tag(&args[0]) != tag(result))
        .count();
    assert!(upgraded > 0, "no case changes");
    for (args, result) in cases {
        let old = tag(&args[0]);
        assert_eq!(
            *block_state_data::upgrade_block_state_tag(&old),
            tag(&result),
            "upgradeBlockStateTag({})",
            args[0]
        );
    }
}

#[test]
fn item_and_block_id_tables_equal_the_recording() {
    use tables::{entity_block_state_fix, item_id_fix, item_stack_the_flattening_fix};
    for (args, result) in rows("ItemIdFix.getItem") {
        let id: i32 = args[0].parse().expect("id");
        assert_eq!(item_id_fix::get_item(id), result, "getItem({id})");
    }
    for (args, result) in rows("EntityBlockStateFix.getBlockId") {
        let name = string(&args[0]);
        assert_eq!(
            entity_block_state_fix::get_block_id(&name).to_string(),
            result,
            "getBlockId({name})"
        );
    }
    let updates = rows("ItemStackTheFlatteningFix.updateItem");
    assert!(
        updates.iter().any(|(_, r)| r != "-"),
        "some updateItem results"
    );
    for (args, result) in updates {
        let data: i32 = args[1].parse().expect("damage");
        assert_eq!(
            item_stack_the_flattening_fix::update_item(&args[0], data),
            java_null_is_none(&result),
            "updateItem({}, {data})",
            args[0]
        );
    }
}

#[test]
fn typed_views_keep_the_dump_order() {
    let mut checked = 0;
    for name in tables::names() {
        for f in &tables::table(name).fields {
            let strings = |t: &Option<Tag>| matches!(t, Some(Tag::String(_)));
            if f.kind == tables::Kind::Map
                && !f.entries.is_empty()
                && f.entries
                    .iter()
                    .all(|e| strings(&e.key) && strings(&e.value))
            {
                let view = f.string_map();
                let keys: Vec<&str> = view.iter().map(|(k, _)| k.as_str()).collect();
                let dumped: Vec<&str> = f
                    .entries
                    .iter()
                    .map(|e| e.key.as_ref().and_then(Tag::as_string).expect("key"))
                    .collect();
                assert_eq!(keys, dumped, "{name}.{}", f.name);
                checked += 1;
            }
        }
    }
    assert!(checked > 100, "{checked} string maps checked");
}
