pub mod above_root_placement;

pub mod acacia_foliage_placer;

pub mod alter_ground_decorator;

pub mod attached_to_leaves_decorator;

pub mod attached_to_logs_decorator;

pub mod beehive_decorator;

pub mod bending_trunk_placer;

pub mod blob_foliage_placer;

pub mod bush_foliage_placer;

pub mod cherry_foliage_placer;

pub mod cherry_trunk_placer;

pub mod cocoa_decorator;

pub mod creaking_heart_decorator;

pub mod dark_oak_foliage_placer;

pub mod dark_oak_trunk_placer;

pub mod fallen_tree_feature;

pub mod fancy_foliage_placer;

pub mod fancy_trunk_placer;

pub mod feature_size;

use feature_size::FeatureSize;

pub mod foliage_placer;

use foliage_placer::FoliagePlacer;

pub mod forking_trunk_placer;

pub mod giant_trunk_placer;

pub mod leave_vine_decorator;

pub mod mangrove_root_placement;

pub mod mangrove_root_placer;

pub mod mega_jungle_foliage_placer;

pub mod mega_jungle_trunk_placer;

pub mod mega_pine_foliage_placer;

pub mod pale_moss_decorator;

pub mod pine_foliage_placer;

pub mod place_on_ground_decorator;

pub mod poplar_foliage_placer;

pub mod poplar_trunk_placer;

pub mod random_spread_foliage_placer;

pub mod root_placer;

use root_placer::RootPlacer;

pub mod shelf_mushroom_decorator;

pub mod spruce_foliage_placer;

pub mod straight_trunk_placer;

pub mod three_layers_feature_size;

pub mod tree_decorator;

use tree_decorator::TreeDecorator;

pub mod tree_feature;

pub mod trunk_placer;

use trunk_placer::TrunkPlacer;

pub mod trunk_vine_decorator;

pub mod two_layers_feature_size;

pub mod upwards_branching_trunk_placer;

use mc_game_core::world::level::levelgen::codec::TypeEntry;

#[linkme::distributed_slice]
pub static TRUNK_PLACER_TYPES: [TypeEntry<dyn TrunkPlacer>];
#[linkme::distributed_slice]
pub static FOLIAGE_PLACER_TYPES: [TypeEntry<dyn FoliagePlacer>];
#[linkme::distributed_slice]
pub static ROOT_PLACER_TYPES: [TypeEntry<dyn RootPlacer>];
#[linkme::distributed_slice]
pub static TREE_DECORATOR_TYPES: [TypeEntry<dyn TreeDecorator>];
#[linkme::distributed_slice]
pub static FEATURE_SIZE_TYPES: [TypeEntry<dyn FeatureSize>];

/// A `state.is(tag)` test with the tag resolved once.
macro_rules! block_tag_fn {
    ($(#[$doc:meta])* $name:ident, $tag:literal) => {
        $(#[$doc])*
        pub fn $name(state: mc_data::block::BlockState) -> bool {
            static TAG: std::sync::OnceLock<Option<&'static mc_data::tags::TagSet<mc_data::block::Block>>> = std::sync::OnceLock::new();
            TAG.get_or_init(|| mc_data::tags::get::<mc_data::block::Block>($tag)).is_some_and(|tag| tag.contains(state.block()))
        }
    };
}
pub(crate) use block_tag_fn;
