//! Pack selection, `level.dat` `DataPacks` round trip and the layered resource manager.

use crate::common;

use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_game::server::packs::resources::{PackResources, PathPackResources};
use mc_game::server::packs::{
    KnownPack, MultiPackResourceManager, PackLocationInfo, PackSource, PackType, ResourceManager, ServerPacksSource, configure_pack_repository,
};
use mc_nbt::CompoundTag;
use mc_text::Component;
use mc_world::level_storage::{DataPackConfig, DirectoryValidator, FeatureFlagSet, WorldDataConfiguration};

fn id(s: &str) -> Identifier {
    Identifier::parse(s).unwrap()
}

#[test]
fn selection_round_trips_through_level_dat() {
    let _s = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    common::init_logger();
    let dir = tempfile::tempdir().unwrap();
    let world = dir.path().join("world");
    common::write_custom_packs(&world);
    let (config, stem) = common::load_world(&world, WorldDataConfiguration::DEFAULT, true).unwrap();
    stem.close();
    let first_logs = common::take_logs();
    assert!(first_logs.iter().any(|l| l.contains("Found new data pack file/good, loading it automatically")));
    let packs = config.data_packs.clone().unwrap();
    // vanilla + the four loadable world packs (selected in sorted id order, vanilla first as listed by the config) ...
    assert_eq!(packs.enabled, ["vanilla", "file/badformat", "file/good", "file/oldformat", "file/zipped.zip"]);
    // ... and everything else (feature packs) disabled
    assert_eq!(packs.disabled, ["minecart_improvements", "redstone_experiments", "trade_rebalance"]);

    // level.dat form: encode, decode, load again with the stored configuration: same selection, nothing "new"
    let mut tag = CompoundTag::new();
    config.encode_into(&mut tag);
    let stored = WorldDataConfiguration::decode(&tag);
    assert_eq!(stored, config);
    let (again, stem) = common::load_world(&world, stored, false).unwrap();
    stem.close();
    assert_eq!(again, config);
    assert!(!common::take_logs().iter().any(|l| l.contains("Found new data pack")));
}

#[test]
fn disabled_missing_and_feature_packs() {
    let _s = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    common::init_logger();
    let dir = tempfile::tempdir().unwrap();
    let world = dir.path().join("world");
    common::write_custom_packs(&world);
    let mut repo = ServerPacksSource::create_pack_repository(world.join("datapacks"), DirectoryValidator::no_symlinks_allowed());
    let _ = common::take_logs();

    // an explicit `DataPacks` list: missing packs are dropped with a warning, listed-disabled packs are not auto-added,
    // a selected pack whose features are not enabled is removed
    let cfg = WorldDataConfiguration {
        data_packs: Some(DataPackConfig {
            enabled: vec!["vanilla".into(), "gone".into(), "trade_rebalance".into()],
            disabled: vec!["file/good".into(), "file/zipped.zip".into()],
        }),
        enabled_features: FeatureFlagSet::VANILLA,
    };
    let out = configure_pack_repository(&mut repo, &cfg, false, false);
    let logs = common::take_logs();
    assert!(logs.contains(&"WARN: Missing data pack gone".to_owned()), "{logs:?}");
    assert!(logs.contains(&"WARN: Pack trade_rebalance requires features minecraft:trade_rebalance that are not enabled for this world, disabling pack.".to_owned()), "{logs:?}");
    let packs = out.data_packs.unwrap();
    assert_eq!(packs.enabled, ["vanilla", "file/badformat", "file/oldformat"]);
    assert!(packs.disabled.contains(&"file/good".to_owned()) && packs.disabled.contains(&"trade_rebalance".to_owned()));

    // enabling the flag lets the feature pack through; its files override the vanilla ones
    let cfg = WorldDataConfiguration {
        data_packs: Some(DataPackConfig { enabled: vec!["vanilla".into(), "trade_rebalance".into()], disabled: vec![] }),
        enabled_features: FeatureFlagSet::VANILLA.join(FeatureFlagSet::TRADE_REBALANCE),
    };
    let (out, stem) = common::load_world(&world, cfg, false).unwrap();
    assert!(out.data_packs.unwrap().enabled.starts_with(&["vanilla".to_owned(), "trade_rebalance".to_owned()]));
    let loot = stem.registries.composite_access().registry("minecraft:loot_table").unwrap().clone();
    let entry = loot.entry_by_key(&id("minecraft:chests/abandoned_mineshaft")).unwrap();
    assert_eq!(entry.source_pack, "trade_rebalance");
    assert_eq!(entry.info.known_pack, Some(KnownPack::vanilla("trade_rebalance")));
    let vanilla_entry = loot.entry_by_key(&id("minecraft:chests/simple_dungeon")).unwrap();
    assert_eq!(vanilla_entry.info.known_pack, Some(KnownPack::vanilla("core")));
    let _ = common::take_logs();

    // new world asking for a feature pack through initial-enabled-packs: its feature is forced on
    let cfg = WorldDataConfiguration {
        data_packs: Some(DataPackConfig { enabled: vec!["vanilla".into(), "redstone_experiments".into()], disabled: vec![] }),
        enabled_features: FeatureFlagSet::VANILLA,
    };
    let out = configure_pack_repository(&mut repo, &cfg, true, false);
    assert!(out.enabled_features.contains_all(FeatureFlagSet::REDSTONE_EXPERIMENTS));
    // safe mode: vanilla only, nothing disabled
    let out = configure_pack_repository(&mut repo, &WorldDataConfiguration::DEFAULT, false, true);
    assert_eq!(out.data_packs.unwrap(), DataPackConfig { enabled: vec!["vanilla".into()], disabled: vec![] });
}

fn path_pack(name: &str, root: &std::path::Path) -> Arc<dyn PackResources> {
    let info = PackLocationInfo { id: name.to_owned(), title: Component::literal(name), source: PackSource::World, known_pack_info: None };
    Arc::new(PathPackResources::new(info, root.to_path_buf()))
}

#[test]
fn resource_manager_layering_and_filters() {
    let dir = tempfile::tempdir().unwrap();
    let write = |p: &str, c: &str| {
        let path = dir.path().join(p);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, c).unwrap();
    };
    write("a/data/ns/things/x.json", "a-x");
    write("a/data/ns/things/only_a.json", "a-only");
    write("a/data/ns/things/x.json.mcmeta", r#"{"animation": 1}"#);
    write("b/data/ns/things/x.json", "b-x");
    write("b/data/ns/things/sub/y.json", "b-y");
    write("b/data/other/things/z.json", "b-z");
    write("c/pack.mcmeta", r#"{"filter": {"block": [{"namespace": "ns", "path": "things/only_a.*"}]}}"#);
    write("c/data/ns/things/c.json", "c-c");
    let mgr = MultiPackResourceManager::new(PackType::ServerData, vec![path_pack("a", &dir.path().join("a")), path_pack("b", &dir.path().join("b")), path_pack("c", &dir.path().join("c"))]);
    assert_eq!(mgr.namespaces().into_iter().collect::<Vec<_>>(), ["ns", "other"]);
    // later packs override earlier ones
    let x = mgr.get_resource(&id("ns:things/x.json")).unwrap();
    assert_eq!((x.source_pack_id(), x.read_all_as_string().unwrap().as_str()), ("b", "b-x"));
    // metadata is only looked up in the winning pack and the packs above it, never below
    assert!(x.metadata().unwrap().section("animation").is_none());
    let stack = mgr.get_resource_stack(&id("ns:things/x.json"));
    assert_eq!(stack.iter().map(|r| r.source_pack_id().to_owned()).collect::<Vec<_>>(), ["a", "b"]);
    assert!(stack[0].metadata().unwrap().section("animation").is_some());
    // `c`'s filter hides `a`'s `only_a`
    assert!(mgr.get_resource(&id("ns:things/only_a.json")).is_none());
    // listing: files only, sorted by (path, namespace), nested dirs included, `.mcmeta` files are not entries
    let listed: Vec<String> = mgr.list_resources("things", &|i| i.path().ends_with(".json")).keys().map(|k| k.to_string()).collect();
    assert_eq!(listed, ["ns:things/c.json", "ns:things/sub/y.json", "ns:things/x.json", "other:things/z.json"]);
    let stacks = mgr.list_resource_stacks("things", &|i| i.path().ends_with(".json"));
    assert_eq!(stacks[&id("ns:things/x.json")].len(), 2);
}

#[test]
fn zip_pack_is_read_like_a_directory_pack() {
    let _s = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let world = dir.path().join("world");
    common::write_custom_packs(&world);
    let mut repo = ServerPacksSource::create_pack_repository(world.join("datapacks"), DirectoryValidator::no_symlinks_allowed());
    repo.reload();
    let zip = repo.get_pack("file/zipped.zip").expect("zip pack discovered");
    assert_eq!(zip.compatibility(), mc_game::server::packs::PackCompatibility::Compatible);
    assert_eq!(repo.get_pack("file/oldformat").unwrap().compatibility(), mc_game::server::packs::PackCompatibility::TooOld);
    assert_eq!(repo.get_pack("file/badformat").unwrap().compatibility(), mc_game::server::packs::PackCompatibility::Unknown);
    assert!(repo.get_pack("file/badmeta").is_none() && repo.get_pack("file/nodesc").is_none() && repo.get_pack("file/nopack").is_none());
    let mgr = MultiPackResourceManager::new(PackType::ServerData, zip.open());
    let r = mgr.get_resource(&id("zip:damage_type/zipped.json")).unwrap();
    assert!(r.read_all_as_string().unwrap().contains("zipped"));
    assert_eq!(mgr.namespaces().into_iter().collect::<Vec<_>>(), ["zip"]);
    // `Pack.Position` of fixed packs and `setSelected`
    repo.set_selected(["vanilla", "file/zipped.zip", "nonexistent"]);
    assert_eq!(repo.selected_ids(), ["vanilla", "file/zipped.zip"]);
    assert!(repo.add_pack("file/oldformat") && !repo.add_pack("file/oldformat") && repo.remove_pack("vanilla"));
    assert_eq!(repo.selected_ids(), ["file/zipped.zip", "file/oldformat"]);
}

#[test]
fn reload_with_new_selection_and_failure_keeps_old_resources() {
    use mc_game::server::reloadable_resources::reload_resources;
    let _s = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let world = dir.path().join("world");
    common::write_custom_packs(&world);
    let (_, stem) = common::load_world(&world, WorldDataConfiguration::DEFAULT, true).unwrap();
    let mut repo = ServerPacksSource::create_pack_repository(world.join("datapacks"), DirectoryValidator::no_symlinks_allowed());
    repo.reload();
    let wool = id("minecraft:wool");
    let block = |layers: &mc_game::registry::LayeredRegistryAccess<mc_game::server::registry_layer::RegistryLayer>| {
        layers.composite_access().registry("minecraft:block").unwrap().tag(&wool).unwrap().len()
    };
    // the world was loaded with `file/good` (wool tag replaced by one block)
    assert_eq!(block(&stem.registries), 1);

    // `/datapack disable file/good` + `/reload`: only vanilla -> the full wool tag again
    let ok = reload_resources(&repo, &["vanilla".to_owned()], &stem.registries, stem.data_pack_resources.enabled_features(), &mc_game::server::permissions::LevelBasedPermissionSet::GAMEMASTER.into()).unwrap();
    assert!(block(ok.managers.registry_layers()) > 10);
    // the world registries (worldgen) are not reloaded
    assert!(ok.managers.registry_layers().composite_access().registry("minecraft:worldgen/biome").unwrap().contains(&id("test:custom")));

    // a pack with a broken reloadable entry: the reload fails and nothing is returned
    let broken = world.join("datapacks/broken");
    std::fs::create_dir_all(broken.join("data/test/recipe")).unwrap();
    std::fs::write(broken.join("pack.mcmeta"), r#"{"pack": {"description": "b", "min_format": 121, "max_format": 121}}"#).unwrap();
    std::fs::write(broken.join("data/test/recipe/bad.json"), "[1, 2").unwrap();
    repo.reload();
    let err = reload_resources(&repo, &["vanilla".to_owned(), "file/broken".to_owned()], &stem.registries, stem.data_pack_resources.enabled_features(), &mc_game::server::permissions::LevelBasedPermissionSet::GAMEMASTER.into());
    assert!(matches!(err, Err(mc_game::server::reloadable_resources::ReloadError::Registries(_))));
}
