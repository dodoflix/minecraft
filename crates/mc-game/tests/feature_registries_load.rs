//! Every vanilla `worldgen/feature`, `worldgen/placed_feature` and `worldgen/block_state_provider` entry decodes (a missing
//! feature/placement/predicate type or an invalid config fails `GenRegistries::load`, as it fails the vanilla server start).

use mc_game::world::level::levelgen::registries::load_vanilla_data;

#[test]
fn vanilla_feature_registries_decode() {
    let (_, _, registries) = load_vanilla_data();
    let registries = registries.unwrap_or_else(|e| panic!("feature registries: {e}"));
    // entry counts of the 26.3 data pack (`ref/src/data/minecraft/worldgen/{feature,placed_feature}`, subdirectories included)
    assert_eq!(registries.features.len(), 240);
    assert_eq!(registries.placed_features.len(), 273);
}
