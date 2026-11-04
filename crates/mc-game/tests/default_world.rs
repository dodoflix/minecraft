//! Default selection of an empty world loads every vanilla registry; tags equal vanilla's `TagLoader` output.
mod common;

use std::collections::BTreeMap;

use mc_core::ident::Identifier;
use mc_game::registry::keys;
use mc_game::server::registry_layer::RegistryLayer;
use mc_world::level_storage::{DataPackConfig, FeatureFlagSet, WorldDataConfiguration};

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn default_world_loads_all_registries_and_tags() {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    common::init_logger();
    let dir = tempfile::tempdir().unwrap();
    let (config, stem) = common::load_world(dir.path(), WorldDataConfiguration::DEFAULT, true).expect("default world loads");
    let logs = common::take_logs();
    // initial selection of a new world: only the vanilla pack (feature packs need their flag)
    assert_eq!(config.data_packs.as_ref().unwrap().enabled, vec!["vanilla".to_owned()], "{logs:?}");
    assert_eq!(config.data_packs.as_ref().unwrap().disabled, vec!["minecart_improvements", "redstone_experiments", "trade_rebalance"]);
    assert_eq!(config.enabled_features, FeatureFlagSet::VANILLA);
    assert!(logs.iter().all(|l| !l.starts_with("ERROR") && !l.starts_with("WARN")), "{logs:?}");

    let access = stem.registries.composite_access();
    // all data-driven registries exist, entries equal the vanilla data pack's files
    for r in keys::WORLD_REGISTRIES.iter().chain(keys::RELOADABLE_REGISTRIES) {
        let registry = access.registry(r.key).unwrap_or_else(|| panic!("{}", r.key));
        let prefix = format!("data/minecraft/{}/", keys::dir_path(r.key));
        let files = mc_data::datapack::paths().filter(|p| p.starts_with(&prefix) && p.ends_with(".json")).count();
        assert_eq!(registry.len(), files, "{}", r.key);
    }
    assert!(access.registry(keys::LEVEL_STEM).is_none() || access.registry(keys::LEVEL_STEM).unwrap().len() == 0);
    let overworld = Identifier::parse("minecraft:overworld").unwrap();
    let dim = access.dimension_type(&overworld).expect("typed dimension type");
    assert_eq!((dim.min_y, dim.height, dim.logical_height), (-64, 384, 384));
    assert!(access.biome(&Identifier::parse("minecraft:plains").unwrap()).is_some());
    assert!(access.damage_type(&Identifier::parse("minecraft:in_fire").unwrap()).is_some());
    assert!(access.chat_type(&Identifier::parse("minecraft:chat").unwrap()).is_some());
    let _ = RegistryLayer::World;

    // static tags equal the extractor's resolved tags
    let expected: BTreeMap<String, BTreeMap<String, Vec<String>>> =
        serde_json::from_slice(&std::fs::read(repo_root().join("ref/extracted/tags.json")).unwrap()).unwrap();
    for (registry_key, tags) in expected {
        let registry = access.registry(&registry_key).expect("static registry");
        assert_eq!(registry.tag_count(), tags.len(), "{registry_key} tag count");
        for (tag, entries) in tags {
            let ids = registry.tag(&Identifier::parse(&tag).unwrap()).unwrap_or_else(|| panic!("{registry_key} {tag}"));
            let mut names: Vec<String> = ids.iter().map(|i| registry.name_of(*i).unwrap().to_string()).collect();
            names.sort();
            let mut want = entries;
            want.sort();
            assert_eq!(names, want, "{registry_key} {tag}");
        }
    }
    let _ = DataPackConfig::default_config();
}
