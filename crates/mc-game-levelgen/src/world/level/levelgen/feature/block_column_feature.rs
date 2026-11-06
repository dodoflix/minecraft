//! `BlockColumnFeature`.

use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::blockpredicates::{self, BlockPredicate};
use mc_game_core::world::level::levelgen::codec::{
    Fields, LoadResult, TypeEntry, parse_bool, parse_direction, parse_list,
};
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature};
use mc_game_core::world::level::levelgen::holder::Holder;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::levelgen::stateproviders::BlockStateProvider;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

/// `BlockColumnFeature.Layer`.
pub struct Layer {
    pub height: IntProvider,
    pub state: Holder<dyn BlockStateProvider>,
}

pub struct BlockColumnFeature {
    pub layers: Vec<Layer>,
    pub direction: Direction,
    pub allowed_placement: Arc<dyn BlockPredicate>,
    pub prioritize_tip: bool,
}

impl BlockColumnFeature {
    pub fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(BlockColumnFeature {
            layers: fields.required("layers", |v| parse_list(v, |l| decode_layer(l, ctx)))?,
            direction: fields.required("direction", parse_direction)?,
            allowed_placement: fields
                .required("allowed_placement", |v| blockpredicates::decode(v, ctx))?,
            prioritize_tip: fields.required("prioritize_tip", parse_bool)?,
        }))
    }
}

fn decode_layer(value: &serde_json::Value, ctx: &DecodeCtx<'_>) -> LoadResult<Layer> {
    let fields = Fields::new(value)?;
    Ok(Layer {
        height: fields.required("height", IntProvider::non_negative_from_json)?,
        state: fields.required("provider", |v| ctx.block_state_provider(v))?,
    })
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static BLOCK_COLUMN: TypeEntry<dyn Feature> = TypeEntry {
    name: "block_column",
    decode: BlockColumnFeature::decode,
};

impl Feature for BlockColumnFeature {
    fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        _generator: &GenEnv,
        random: &mut dyn RandomSource,
        origin: BlockPos,
    ) -> bool {
        let layer_count = self.layers.len();
        let mut layer_heights = vec![0i32; layer_count];
        let mut total_height = 0i32;
        for i in 0..layer_count {
            layer_heights[i] = self.layers[i].height.sample(random);
            total_height = total_height.wrapping_add(layer_heights[i]);
        }

        if total_height == 0 {
            return false;
        }
        let mut place_pos = origin;
        let mut next_pos = origin.relative(self.direction);
        for y in 0..total_height {
            if !self.allowed_placement.test(&*level, next_pos) {
                truncate(&mut layer_heights, total_height, y, self.prioritize_tip);
                break;
            }
            next_pos = next_pos.relative(self.direction);
        }

        for i in 0..layer_count {
            let count = layer_heights[i];
            if count != 0 {
                let layer = &self.layers[i];
                for _ in 0..count {
                    let state = layer.state.value().get_state(&*level, random, place_pos);
                    level.set_block_flags(place_pos, state, 2);
                    place_pos = place_pos.relative(self.direction);
                }
            }
        }
        true
    }
}

fn truncate(layer_heights: &mut [i32], total_height: i32, new_height: i32, prioritize_tip: bool) {
    let mut amount_to_remove = total_height - new_height;
    let direction: i32 = if prioritize_tip { 1 } else { -1 };
    let start: i32 = if prioritize_tip {
        0
    } else {
        layer_heights.len() as i32 - 1
    };
    let end: i32 = if prioritize_tip {
        layer_heights.len() as i32
    } else {
        -1
    };
    let mut i = start;
    while i != end && amount_to_remove > 0 {
        let this_layer = layer_heights[i as usize];
        let to_remove_from_layer = this_layer.min(amount_to_remove);
        amount_to_remove -= to_remove_from_layer;
        layer_heights[i as usize] -= to_remove_from_layer;
        i += direction;
    }
}

#[cfg(test)]
mod tests {
    use super::truncate;

    #[test]
    fn truncate_removes_from_tip_or_base() {
        let mut heights = [3, 2, 4];
        truncate(&mut heights, 9, 2, false);
        assert_eq!(heights, [2, 0, 0]);
        let mut heights = [3, 2, 4];
        truncate(&mut heights, 9, 2, true);
        assert_eq!(heights, [0, 0, 2]);
    }
}
