//! `SpikeFeature` (`spike`): ice spikes.

use super::{FEATURE_TYPES, Feature, set_block};
use crate::world::level::levelgen::GenEnv;
use crate::world::level::levelgen::WorldGenLevel;
use crate::world::level::levelgen::blockpredicates::{self, BlockPredicate};
use crate::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry, parse_block_state};
use crate::world::level::levelgen::registries::DecodeCtx;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use std::sync::Arc;

pub struct SpikeFeature {
    pub state: BlockState,
    pub can_place_on: Arc<dyn BlockPredicate>,
    pub can_replace: Arc<dyn BlockPredicate>,
}

impl SpikeFeature {
    pub fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(SpikeFeature {
            state: fields.required("state", parse_block_state)?,
            can_place_on: fields.required("can_place_on", |v| blockpredicates::decode(v, ctx))?,
            can_replace: fields.required("can_replace", |v| blockpredicates::decode(v, ctx))?,
        }))
    }
}

impl Feature for SpikeFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, mut origin: BlockPos) -> bool {
        while level.is_empty_block(origin) && origin.y() > level.get_min_y() + 2 {
            origin = origin.below();
        }
        if !self.can_place_on.test(level, origin) {
            return false;
        }
        origin = origin.above_n(random.next_int_bounded(4));
        let height = random.next_int_bounded(4) + 7;
        let width = height / 4 + random.next_int_bounded(2);
        if width > 1 && random.next_int_bounded(60) == 0 {
            origin = origin.above_n(10 + random.next_int_bounded(30));
        }

        for y_off in 0..height {
            let scale = (1.0f32 - y_off as f32 / height as f32) * width as f32;
            let new_width = mth::ceil_f32(scale);
            for xo in -new_width..=new_width {
                let dx = xo.abs() as f32 - 0.25;
                for zo in -new_width..=new_width {
                    let dz = zo.abs() as f32 - 0.25;
                    if ((xo == 0 && zo == 0) || !(dx * dx + dz * dz > scale * scale))
                        && ((xo != -new_width && xo != new_width && zo != -new_width && zo != new_width) || !(random.next_float() > 0.75))
                    {
                        let positive_offset = origin.offset(xo, y_off, zo);
                        let state = level.get_block_state(positive_offset);
                        if state.is_air() || self.can_replace.test(level, positive_offset) {
                            set_block(level, positive_offset, self.state);
                        }
                        if y_off != 0 && new_width > 1 {
                            let negative_offset = origin.offset(xo, -y_off, zo);
                            let state = level.get_block_state(negative_offset);
                            if state.is_air() || self.can_replace.test(level, negative_offset) {
                                set_block(level, negative_offset, self.state);
                            }
                        }
                    }
                }
            }
        }

        let pillar_width = (width - 1).clamp(0, 1);
        for xo in -pillar_width..=pillar_width {
            for zo in -pillar_width..=pillar_width {
                let mut cursor = origin.offset(xo, -1, zo);
                let mut run_length = 50;
                if xo.abs() == 1 && zo.abs() == 1 {
                    run_length = random.next_int_bounded(5);
                }
                while cursor.y() > 50 {
                    let state = level.get_block_state(cursor);
                    if !state.is_air() && !self.can_replace.test(level, cursor) && state != self.state {
                        break;
                    }
                    set_block(level, cursor, self.state);
                    cursor = cursor.below();
                    run_length -= 1;
                    if run_length <= 0 {
                        cursor = cursor.below_n(random.next_int_bounded(5) + 1);
                        run_length = random.next_int_bounded(5);
                    }
                }
            }
        }
        true
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static SPIKE: TypeEntry<dyn Feature> = TypeEntry { name: "spike", decode: SpikeFeature::decode };
