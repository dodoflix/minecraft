//! `DiskFeature`: a flat disc of blocks (sand/clay/gravel patches) replacing the blocks its predicate accepts.

use super::{Feature, mark_above_for_post_processing};
use crate::world::level::levelgen::blockpredicates::{self, BlockPredicate};
use crate::world::level::levelgen::codec::{TypeEntry, parse_int_range};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::stateproviders::BlockStateProvider;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

pub struct DiskFeature {
    pub state_provider: Holder<dyn BlockStateProvider>,
    pub target: Arc<dyn BlockPredicate>,
    pub radius: IntProvider,
    pub half_height: i32,
}

impl Feature for DiskFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let mut placed_any = false;
        let origin_y = origin.y();
        let top = origin_y + self.half_height;
        let bottom = origin_y - self.half_height - 1;
        let r = self.radius.sample(random);

        for column_pos in BlockPos::between_closed(&origin.offset(-r, 0, -r), &origin.offset(r, 0, r)) {
            let xd = column_pos.x() - origin.x();
            let zd = column_pos.z() - origin.z();
            if xd * xd + zd * zd <= r * r {
                placed_any |= self.place_column(level, random, top, bottom, column_pos);
            }
        }
        placed_any
    }
}

impl DiskFeature {
    fn place_column(&self, level: &mut dyn WorldGenLevel, random: &mut dyn RandomSource, top: i32, bottom: i32, column_pos: BlockPos) -> bool {
        let mut placed_any = false;
        let mut placed_above = false;

        for y in (bottom + 1..=top).rev() {
            let pos = column_pos.at_y(y);
            if self.target.test(&*level, pos) {
                if let Some(state) = self.state_provider.value().get_optional_state(&*level, random, pos) {
                    level.set_block_flags(pos, state, 2);
                    if !placed_above {
                        mark_above_for_post_processing(level, pos);
                    }
                    placed_any = true;
                    placed_above = true;
                }
            } else {
                placed_above = false;
            }
        }
        placed_any
    }
}

#[linkme::distributed_slice(super::FEATURE_TYPES)]
static DISK: TypeEntry<dyn Feature> = TypeEntry {
    name: "disk",
    decode: |fields, ctx| {
        Ok(Arc::new(DiskFeature {
            state_provider: fields.required("state_provider", |v| ctx.block_state_provider(v))?,
            target: fields.required("target", |v| blockpredicates::decode(v, ctx))?,
            radius: fields.required("radius", |v| IntProvider::from_json_range(v, 0, 8))?,
            half_height: fields.required("half_height", |v| parse_int_range(v, 0, 4))?,
        }))
    },
};
