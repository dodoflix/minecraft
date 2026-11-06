//! `CountOnEveryLayerPlacement`: `count` random columns per ground layer, for every layer that has one.

use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry};
use mc_game_core::world::level::levelgen::placement::{
    PLACEMENT_MODIFIER_TYPES, PlacementContext, PlacementModifier,
};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_world::heightmap::HeightmapType;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

pub struct CountOnEveryLayerPlacement {
    pub count: IntProvider,
}

impl CountOnEveryLayerPlacement {
    fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PlacementModifier>> {
        Ok(Arc::new(CountOnEveryLayerPlacement {
            count: fields.required("count", |v| IntProvider::from_json_range(v, 0, 256))?,
        }))
    }

    fn find_on_ground_y_position(
        context: &PlacementContext<'_>,
        x_start: i32,
        y_start: i32,
        z_start: i32,
        layer_to_place_on: i32,
    ) -> i32 {
        let mut current_layer = 0;
        let mut current_block = context.get_block_state(BlockPos::new(x_start, y_start, z_start));
        let mut y = y_start;
        while y >= context.get_min_y() + 1 {
            let below_pos = BlockPos::new(x_start, y - 1, z_start);
            let below_block = context.get_block_state(below_pos);
            if !Self::is_empty(below_block)
                && Self::is_empty(current_block)
                && !below_block.is_block(Block::BEDROCK)
            {
                if current_layer == layer_to_place_on {
                    return below_pos.y() + 1;
                }
                current_layer += 1;
            }
            current_block = below_block;
            y -= 1;
        }
        i32::MAX
    }

    fn is_empty(state: BlockState) -> bool {
        state.is_air() || state.is_block(Block::WATER) || state.is_block(Block::LAVA)
    }
}

impl PlacementModifier for CountOnEveryLayerPlacement {
    fn modify(
        &self,
        context: &PlacementContext<'_>,
        random: &mut dyn RandomSource,
        origin: BlockPos,
        output: &mut Vec<BlockPos>,
    ) {
        let mut layer = 0;
        loop {
            let mut found_any = false;
            let mut i = 0;
            // the provider is re-sampled on every loop test, like vanilla
            while i < self.count.sample(random) {
                let x = random.next_int_bounded(16).wrapping_add(origin.x());
                let z = random.next_int_bounded(16).wrapping_add(origin.z());
                let start_y = context.get_height(HeightmapType::MotionBlocking, x, z);
                let y = Self::find_on_ground_y_position(context, x, start_y, z, layer);
                if y != i32::MAX {
                    output.push(BlockPos::new(x, y, z));
                    found_any = true;
                }
                i += 1;
            }
            layer += 1;
            if !found_any {
                break;
            }
        }
    }
}

#[linkme::distributed_slice(PLACEMENT_MODIFIER_TYPES)]
static COUNT_ON_EVERY_LAYER: TypeEntry<dyn PlacementModifier> = TypeEntry {
    name: "count_on_every_layer",
    decode: CountOnEveryLayerPlacement::decode,
};
