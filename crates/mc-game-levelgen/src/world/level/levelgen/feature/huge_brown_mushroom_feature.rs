//! `HugeBrownMushroomFeature`.

use super::abstract_huge_mushroom_feature::{self, AbstractHugeMushroomFeature};
use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature};
use mc_game_core::world::level::levelgen::blockpredicates::{self, BlockPredicate};
use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry, parse_int};
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::holder::Holder;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::levelgen::stateproviders::BlockStateProvider;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::props;
use std::sync::Arc;

pub struct HugeBrownMushroomFeature {
    pub cap_provider: Holder<dyn BlockStateProvider>,
    pub stem_provider: Holder<dyn BlockStateProvider>,
    pub foliage_radius: i32,
    pub can_place_on: Arc<dyn BlockPredicate>,
}

impl HugeBrownMushroomFeature {
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(HugeBrownMushroomFeature {
            cap_provider: fields.required("cap_provider", |v| ctx.block_state_provider(v))?,
            stem_provider: fields.required("stem_provider", |v| ctx.block_state_provider(v))?,
            foliage_radius: fields.optional("foliage_radius", parse_int)?.unwrap_or(2),
            can_place_on: fields.required("can_place_on", |v| blockpredicates::decode(v, ctx))?,
        }))
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static HUGE_BROWN_MUSHROOM: TypeEntry<dyn Feature> = TypeEntry { name: "huge_brown_mushroom", decode: HugeBrownMushroomFeature::decode };

impl AbstractHugeMushroomFeature for HugeBrownMushroomFeature {
    fn cap_provider(&self) -> &Holder<dyn BlockStateProvider> {
        &self.cap_provider
    }

    fn stem_provider(&self) -> &Holder<dyn BlockStateProvider> {
        &self.stem_provider
    }

    fn foliage_radius(&self) -> i32 {
        self.foliage_radius
    }

    fn can_place_on(&self) -> &dyn BlockPredicate {
        &*self.can_place_on
    }

    fn make_cap(&self, level: &mut dyn WorldGenLevel, random: &mut dyn RandomSource, origin: BlockPos, tree_height: i32) {
        let r = self.foliage_radius;
        for dx in -r..=r {
            for dz in -r..=r {
                let min_x = dx == -r;
                let max_x = dx == r;
                let min_z = dz == -r;
                let max_z = dz == r;
                let x_edge = min_x || max_x;
                let z_edge = min_z || max_z;
                if !x_edge || !z_edge {
                    let block_pos = origin.offset(dx, tree_height, dz);
                    let west = min_x || z_edge && dx == 1 - r;
                    let east = max_x || z_edge && dx == r - 1;
                    let north = min_z || x_edge && dz == 1 - r;
                    let south = max_z || x_edge && dz == r - 1;
                    let mut state = self.cap_provider.value().get_state(&*level, random, origin);
                    if state.has_property(props::WEST) && state.has_property(props::EAST) && state.has_property(props::NORTH) && state.has_property(props::SOUTH) {
                        state = state.set(props::WEST, west).set(props::EAST, east).set(props::NORTH, north).set(props::SOUTH, south);
                    }
                    self.place_mushroom_block(level, block_pos, state);
                }
            }
        }
    }

    fn get_tree_radius_for_height(&self, _trunk_height: i32, _tree_height: i32, leaf_radius: i32, yo: i32) -> i32 {
        if yo <= 3 { 0 } else { leaf_radius }
    }
}

impl Feature for HugeBrownMushroomFeature {
    fn huge_mushroom_foliage_radius(&self) -> Option<i32> {
        Some(self.foliage_radius)
    }

    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        abstract_huge_mushroom_feature::place(self, level, random, origin)
    }
}
