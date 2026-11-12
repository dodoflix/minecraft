//! Every recipe of the vanilla data pack loads through the `recipe` registry decoder and the `RecipeManager`.

use crate::common;

use mc_game::registry::keys;
use mc_game::world::item::crafting::{Recipe, RecipeManager};
use mc_world::level_storage::WorldDataConfiguration;

/// The vanilla data pack's recipes per serializer: 1763 files plus 279 brewing recipes, 21 of the 22 serializers.
const RECIPES_PER_SERIALIZER: &[(&str, usize)] = &[
    ("minecraft:blasting", 25),
    ("minecraft:brewing", 279),
    ("minecraft:campfire_cooking", 9),
    ("minecraft:crafting_decorated_pot", 1),
    ("minecraft:crafting_dye", 6),
    ("minecraft:crafting_imbue", 1),
    ("minecraft:crafting_shaped", 827),
    ("minecraft:crafting_shapeless", 375),
    ("minecraft:crafting_special_bannerduplicate", 16),
    ("minecraft:crafting_special_bookcloning", 1),
    ("minecraft:crafting_special_firework_rocket", 1),
    ("minecraft:crafting_special_firework_star", 1),
    ("minecraft:crafting_special_firework_star_fade", 1),
    ("minecraft:crafting_special_mapextending", 1),
    ("minecraft:crafting_special_repairitem", 1),
    ("minecraft:crafting_special_shielddecoration", 1),
    ("minecraft:crafting_transmute", 33),
    ("minecraft:smelting", 73),
    ("minecraft:smithing_transform", 12),
    ("minecraft:smithing_trim", 18),
    ("minecraft:smoking", 9),
    ("minecraft:stonecutting", 351),
];

#[test]
fn all_vanilla_recipes_load() {
    let _s = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (_, stem) = common::load_world(
        &dir.path().join("world"),
        WorldDataConfiguration::DEFAULT,
        true,
    )
    .unwrap();
    let registry = stem
        .registries
        .composite_access()
        .registry_or_throw(keys::RECIPE);
    let on_disk = std::fs::read_dir(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../ref/src/data/minecraft/recipe"
    ))
    .unwrap()
    .count();
    assert!(registry.len() >= on_disk);
    let typed = registry
        .entries()
        .iter()
        .filter(|e| {
            e.value
                .as_ref()
                .is_some_and(|v| v.downcast_ref::<Recipe>().is_some())
        })
        .count();
    assert_eq!(typed, registry.len());
    let manager = stem
        .data_pack_resources
        .listener::<RecipeManager>()
        .expect("recipe manager listener");
    assert_eq!(manager.recipes().len(), registry.len());
    let mut by_serializer = std::collections::BTreeMap::<&str, usize>::new();
    for holder in manager.recipes() {
        *by_serializer
            .entry(holder.value.serializer().name())
            .or_default() += 1;
    }
    let counts: Vec<_> = by_serializer.into_iter().collect();
    assert_eq!(counts, RECIPES_PER_SERIALIZER);
    assert_eq!(manager.recipes().len(), 2042);
    assert_eq!(manager.learnable_recipes().len(), 1739);
    assert_eq!(manager.stonecutter_recipes().size(), 351);
    stem.close();
}
