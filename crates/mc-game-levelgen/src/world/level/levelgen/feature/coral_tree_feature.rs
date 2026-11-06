//! `CoralTreeFeature`.

use mc_core::direction::{Direction, Plane};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry};
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature};
use mc_game_core::world::level::levelgen::holder::Holder;
use mc_game_core::world::level::levelgen::placement::PlacedFeature;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use std::sync::Arc;

pub struct CoralTreeFeature {
    pub feature: Holder<PlacedFeature>,
}

impl CoralTreeFeature {
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(CoralTreeFeature {
            feature: fields.required("feature", |v| ctx.placed_feature(v))?,
        }))
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static CORAL_TREE: TypeEntry<dyn Feature> = TypeEntry {
    name: "coral_tree",
    decode: CoralTreeFeature::decode,
};

impl Feature for CoralTreeFeature {
    fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        generator: &GenEnv,
        random: &mut dyn RandomSource,
        origin: BlockPos,
    ) -> bool {
        let feature = self.feature.value();
        let mut mut_pos = origin;
        let trunck_height = random.next_int_bounded(3) + 1;

        for _ in 0..trunck_height {
            if !feature.place(level, generator, random, mut_pos) {
                return true;
            }
            mut_pos = mut_pos.relative(Direction::Up);
        }

        let trunck_top_pos = mut_pos;
        let n_branches = random.next_int_bounded(3) + 2;
        let directions = Plane::Horizontal.shuffled_copy(random);

        for &branch_direction in &directions[..n_branches as usize] {
            mut_pos = trunck_top_pos.relative(branch_direction);
            let branch_height = random.next_int_bounded(5) + 2;
            let mut segment_length = 0;

            let mut j = 0;
            while j < branch_height && feature.place(level, generator, random, mut_pos) {
                segment_length += 1;
                mut_pos = mut_pos.relative(Direction::Up);
                if j == 0 || segment_length >= 2 && random.next_float() < 0.25 {
                    mut_pos = mut_pos.relative(branch_direction);
                    segment_length = 0;
                }
                j += 1;
            }
        }
        true
    }
}
