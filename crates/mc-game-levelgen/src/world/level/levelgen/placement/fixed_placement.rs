//! `FixedPlacement`: the listed positions that lie in the origin's chunk.

use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::{
    Fields, LoadResult, TypeEntry, parse_block_pos, parse_list,
};
use mc_game_core::world::level::levelgen::placement::{
    PLACEMENT_MODIFIER_TYPES, PlacementContext, PlacementModifier,
};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use std::sync::Arc;

pub struct FixedPlacement {
    pub positions: Vec<BlockPos>,
}

impl FixedPlacement {
    fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PlacementModifier>> {
        Ok(Arc::new(FixedPlacement {
            positions: fields.required("positions", |v| parse_list(v, parse_block_pos))?,
        }))
    }
}

impl PlacementModifier for FixedPlacement {
    fn modify(
        &self,
        _context: &PlacementContext<'_>,
        _random: &mut dyn RandomSource,
        origin: BlockPos,
        output: &mut Vec<BlockPos>,
    ) {
        let chunk_x = origin.x() >> 4;
        let chunk_z = origin.z() >> 4;
        output.extend(
            self.positions
                .iter()
                .filter(|p| chunk_x == p.x() >> 4 && chunk_z == p.z() >> 4),
        );
    }
}

#[linkme::distributed_slice(PLACEMENT_MODIFIER_TYPES)]
static FIXED_PLACEMENT: TypeEntry<dyn PlacementModifier> = TypeEntry {
    name: "fixed_placement",
    decode: FixedPlacement::decode,
};
