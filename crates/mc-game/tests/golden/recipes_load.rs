//! Every recipe of the vanilla data pack loads through the `recipe` registry decoder and the `RecipeManager`.

use crate::common;

use mc_game::registry::keys;
use mc_game::world::item::crafting::{Recipe, RecipeManager};
use mc_world::level_storage::WorldDataConfiguration;

#[test]
fn all_vanilla_recipes_load() {
    let _s = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (_, stem) = common::load_world(&dir.path().join("world"), WorldDataConfiguration::DEFAULT, true).unwrap();
    let registry = stem.registries.composite_access().registry_or_throw(keys::RECIPE);
    let on_disk = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../ref/src/data/minecraft/recipe")).unwrap().count();
    assert!(registry.len() >= on_disk);
    let typed = registry.entries().iter().filter(|e| e.value.as_ref().is_some_and(|v| v.downcast_ref::<Recipe>().is_some())).count();
    assert_eq!(typed, registry.len());
    let manager = stem.data_pack_resources.listener::<RecipeManager>().expect("recipe manager listener");
    assert_eq!(manager.recipes().len(), registry.len());
    eprintln!("recipes: {} learnable: {} stonecutter: {}", manager.recipes().len(), manager.learnable_recipes().len(), manager.stonecutter_recipes().size());
    stem.close();
}
