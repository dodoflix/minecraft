//! `HugeRedMushroomFeature`.

use super::abstract_huge_mushroom_feature::{self, AbstractHugeMushroomFeature};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::props;
use mc_game_core::world::level::levelgen::blockpredicates::{self, BlockPredicate};
use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry, parse_int};
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature};
use mc_game_core::world::level::levelgen::holder::Holder;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::levelgen::stateproviders::BlockStateProvider;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use std::sync::Arc;

pub struct HugeRedMushroomFeature {
    pub cap_provider: Holder<dyn BlockStateProvider>,
    pub stem_provider: Holder<dyn BlockStateProvider>,
    pub foliage_radius: i32,
    pub can_place_on: Arc<dyn BlockPredicate>,
}

impl HugeRedMushroomFeature {
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(HugeRedMushroomFeature {
            cap_provider: fields.required("cap_provider", |v| ctx.block_state_provider(v))?,
            stem_provider: fields.required("stem_provider", |v| ctx.block_state_provider(v))?,
            foliage_radius: fields.optional("foliage_radius", parse_int)?.unwrap_or(2),
            can_place_on: fields.required("can_place_on", |v| blockpredicates::decode(v, ctx))?,
        }))
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static HUGE_RED_MUSHROOM: TypeEntry<dyn Feature> = TypeEntry {
    name: "huge_red_mushroom",
    decode: HugeRedMushroomFeature::decode,
};

impl AbstractHugeMushroomFeature for HugeRedMushroomFeature {
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

    fn make_cap(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        origin: BlockPos,
        tree_height: i32,
    ) {
        for dy in tree_height - 3..=tree_height {
            let radius = if dy < tree_height {
                self.foliage_radius
            } else {
                self.foliage_radius - 1
            };
            let center = self.foliage_radius - 2;

            for dx in -radius..=radius {
                for dz in -radius..=radius {
                    let min_x = dx == -radius;
                    let max_x = dx == radius;
                    let min_z = dz == -radius;
                    let max_z = dz == radius;
                    let x_edge = min_x || max_x;
                    let z_edge = min_z || max_z;
                    if dy >= tree_height || x_edge != z_edge {
                        let block_pos = origin.offset(dx, dy, dz);
                        let mut state =
                            self.cap_provider.value().get_state(&*level, random, origin);
                        if state.has_property(props::WEST)
                            && state.has_property(props::EAST)
                            && state.has_property(props::NORTH)
                            && state.has_property(props::SOUTH)
                            && state.has_property(props::UP)
                        {
                            state = state
                                .set(props::UP, dy >= tree_height - 1)
                                .set(props::WEST, dx < -center)
                                .set(props::EAST, dx > center)
                                .set(props::NORTH, dz < -center)
                                .set(props::SOUTH, dz > center);
                        }
                        self.place_mushroom_block(level, block_pos, state);
                    }
                }
            }
        }
    }

    fn get_tree_radius_for_height(
        &self,
        _trunk_height: i32,
        tree_height: i32,
        leaf_radius: i32,
        yo: i32,
    ) -> i32 {
        let mut radius = 0;
        if (yo < tree_height && yo >= tree_height - 3) || yo == tree_height {
            radius = leaf_radius;
        }
        radius
    }
}

impl Feature for HugeRedMushroomFeature {
    fn huge_mushroom_foliage_radius(&self) -> Option<i32> {
        Some(self.foliage_radius)
    }

    fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        _generator: &GenEnv,
        random: &mut dyn RandomSource,
        origin: BlockPos,
    ) -> bool {
        abstract_huge_mushroom_feature::place(self, level, random, origin)
    }
}
