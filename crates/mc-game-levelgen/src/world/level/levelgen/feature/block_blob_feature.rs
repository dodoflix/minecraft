//! `BlockBlobFeature`: small mossy-rock style blobs dropped onto the first block the predicate accepts.

use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_game_core::world::level::levelgen::blockpredicates::{self, BlockPredicate};
use mc_game_core::world::level::levelgen::codec::{TypeEntry, parse_block_state};
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::feature::Feature;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use std::sync::Arc;

pub struct BlockBlobFeature {
    pub state: BlockState,
    pub can_place_on: Arc<dyn BlockPredicate>,
}

impl Feature for BlockBlobFeature {
    fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        _generator: &GenEnv,
        random: &mut dyn RandomSource,
        mut origin: BlockPos,
    ) -> bool {
        while origin.y() > level.get_min_y() + 3 && !self.can_place_on.test(&*level, origin.below())
        {
            origin = origin.below();
        }
        if origin.y() <= level.get_min_y() + 3 {
            return false;
        }
        for _ in 0..3 {
            let xr = random.next_int_bounded(2);
            let yr = random.next_int_bounded(2);
            let zr = random.next_int_bounded(2);
            let tr = (xr + yr + zr) as f32 * 0.333 + 0.5;

            for block_pos in
                BlockPos::between_closed(&origin.offset(-xr, -yr, -zr), &origin.offset(xr, yr, zr))
            {
                if block_pos.dist_sqr(&origin) <= f64::from(tr * tr) {
                    level.set_block_and_update(block_pos, self.state);
                }
            }
            origin = origin.offset(
                -1 + random.next_int_bounded(2),
                -random.next_int_bounded(2),
                -1 + random.next_int_bounded(2),
            );
        }
        true
    }
}

#[linkme::distributed_slice(mc_game_core::world::level::levelgen::feature::FEATURE_TYPES)]
static BLOCK_BLOB: TypeEntry<dyn Feature> = TypeEntry {
    name: "block_blob",
    decode: |fields, ctx| {
        Ok(Arc::new(BlockBlobFeature {
            state: fields.required("state", parse_block_state)?,
            can_place_on: fields.required("can_place_on", |v| blockpredicates::decode(v, ctx))?,
        }))
    },
};
