//! `foliageplacers.RandomSpreadFoliagePlacer`.

use super::FOLIAGE_PLACER_TYPES;
use super::foliage_placer::try_place_leaf;
use super::foliage_placer::{FoliageAttachment, FoliageBase, FoliagePlacer};
use super::tree_feature::{TreeFeature, TreeSetter};
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::codec::parse_int_range;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

pub struct RandomSpreadFoliagePlacer {
    base: FoliageBase,
    foliage_height: IntProvider,
    leaf_placement_attempts: i32,
}

impl FoliagePlacer for RandomSpreadFoliagePlacer {
    fn base(&self) -> &FoliageBase {
        &self.base
    }

    fn create_foliage_with_offset(
        &self,
        level: &mut dyn WorldGenLevel,
        foliage_setter: &mut TreeSetter,
        random: &mut dyn RandomSource,
        tree: &TreeFeature,
        _tree_height: i32,
        foliage_attachment: FoliageAttachment,
        foliage_height: i32,
        leaf_radius: i32,
        _offset: i32,
    ) {
        let origin = foliage_attachment.pos;
        for _ in 0..self.leaf_placement_attempts {
            let dx = random.next_int_bounded(leaf_radius) - random.next_int_bounded(leaf_radius);
            let dy =
                random.next_int_bounded(foliage_height) - random.next_int_bounded(foliage_height);
            let dz = random.next_int_bounded(leaf_radius) - random.next_int_bounded(leaf_radius);
            try_place_leaf(
                level,
                foliage_setter,
                random,
                tree,
                origin.offset(dx, dy, dz),
            );
        }
    }

    fn foliage_height(
        &self,
        random: &mut dyn RandomSource,
        _tree_height: i32,
        _tree: &TreeFeature,
    ) -> i32 {
        self.foliage_height.sample(random)
    }

    fn should_skip_location(
        &self,
        _random: &mut dyn RandomSource,
        _dx: i32,
        _y: i32,
        _dz: i32,
        _current_radius: i32,
        _double_trunk: bool,
    ) -> bool {
        false
    }
}

#[linkme::distributed_slice(FOLIAGE_PLACER_TYPES)]
static RANDOM_SPREAD_FOLIAGE_PLACER: TypeEntry<dyn FoliagePlacer> = TypeEntry {
    name: "random_spread_foliage_placer",
    decode: |fields, _| {
        let base = FoliageBase::decode(fields)?;
        let foliage_height = fields.required("foliage_height", |v| {
            IntProvider::from_json_range(v, 1, 512)
        })?;
        let leaf_placement_attempts =
            fields.required("leaf_placement_attempts", |v| parse_int_range(v, 0, 256))?;
        Ok(Arc::new(RandomSpreadFoliagePlacer {
            base,
            foliage_height,
            leaf_placement_attempts,
        }))
    },
};
