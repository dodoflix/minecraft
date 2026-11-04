//! `CoralClawFeature`.

use super::{FEATURE_TYPES, Feature};
use crate::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::java_util::shuffle;
use crate::world::level::levelgen::placement::PlacedFeature;
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::direction::{Direction, Plane};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct CoralClawFeature {
    pub feature: Holder<PlacedFeature>,
}

impl CoralClawFeature {
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(CoralClawFeature { feature: fields.required("feature", |v| ctx.placed_feature(v))? }))
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static CORAL_CLAW: TypeEntry<dyn Feature> = TypeEntry { name: "coral_claw", decode: CoralClawFeature::decode };

impl Feature for CoralClawFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let feature = self.feature.value();
        if !feature.place(level, generator, random, origin) {
            return false;
        }
        let claw_direction = Plane::Horizontal.get_random_direction(random);
        let n_branches = random.next_int_bounded(2) + 2;
        let mut possible_directions = [claw_direction, claw_direction.get_clockwise(), claw_direction.get_counter_clockwise()];
        shuffle(&mut possible_directions, random);

        for &branch_direction in &possible_directions[..n_branches as usize] {
            let mut mut_pos = origin;
            let sideway_length = random.next_int_bounded(2) + 1;
            mut_pos = mut_pos.relative(branch_direction);
            let inway_length;
            let segment_direction;
            if branch_direction == claw_direction {
                segment_direction = claw_direction;
                inway_length = random.next_int_bounded(3) + 2;
            } else {
                mut_pos = mut_pos.relative(Direction::Up);
                let segment_possible_directions = [branch_direction, Direction::Up];
                segment_direction = segment_possible_directions[random.next_int_bounded(2) as usize];
                inway_length = random.next_int_bounded(3) + 3;
            }

            let mut i = 0;
            while i < sideway_length && feature.place(level, generator, random, mut_pos) {
                mut_pos = mut_pos.relative(segment_direction);
                i += 1;
            }

            mut_pos = mut_pos.relative(segment_direction.get_opposite());
            mut_pos = mut_pos.relative(Direction::Up);

            for _ in 0..inway_length {
                mut_pos = mut_pos.relative(claw_direction);
                if !feature.place(level, generator, random, mut_pos) {
                    break;
                }
                if random.next_float() < 0.25 {
                    mut_pos = mut_pos.relative(Direction::Up);
                }
            }
        }
        true
    }
}
