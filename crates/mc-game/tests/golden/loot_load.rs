//! Every vanilla loot data element decodes into its typed value and validates without problems.

use crate::common;

use std::sync::Arc;

use mc_game::registry::keys;
use mc_game::world::level::storage::loot::LootTable;
use mc_game::world::level::storage::loot::loading::validate_loot_registries;
use mc_world::level_storage::WorldDataConfiguration;

#[test]
fn vanilla_loot_registries_decode_and_validate() {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    common::init_logger();
    let dir = tempfile::tempdir().unwrap();
    let (_config, stem) = common::load_world(dir.path(), WorldDataConfiguration::DEFAULT, true)
        .expect("default world loads");
    let access = stem.registries.composite_access();
    for key in [
        keys::LOOT_TABLE,
        keys::PREDICATE,
        keys::ITEM_MODIFIER,
        keys::CONTEXT_INT_PROVIDER,
        keys::CONTEXT_FLOAT_PROVIDER,
    ] {
        let registry = access.registry(key).unwrap();
        assert!(!registry.is_empty() || key == keys::ITEM_MODIFIER, "{key}");
        for entry in registry.entries() {
            assert!(
                entry.value.is_some(),
                "{key} {} has no typed value",
                entry.id
            );
        }
    }
    let tables = access.registry(keys::LOOT_TABLE).unwrap();
    assert_eq!(tables.len(), 1447);
    let stone = tables.get::<Arc<LootTable>>(
        &mc_core::ident::Identifier::parse("minecraft:blocks/stone").unwrap(),
    );
    assert!(stone.is_some());
    validate_loot_registries(access).expect("validation");
}
