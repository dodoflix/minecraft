//! `OffsetPlacement`: the origin shifted by sampled offsets (sampled in x, y, z order).

use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry};
use mc_game_core::world::level::levelgen::placement::{
    PLACEMENT_MODIFIER_TYPES, PlacementContext, PlacementModifier,
};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

pub struct OffsetPlacement {
    pub x: IntProvider,
    pub y: IntProvider,
    pub z: IntProvider,
}

impl OffsetPlacement {
    fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PlacementModifier>> {
        let range = |v: &serde_json::Value| IntProvider::from_json_range(v, -16, 16);
        Ok(Arc::new(OffsetPlacement {
            x: fields.required("x", range)?,
            y: fields.required("y", range)?,
            z: fields.required("z", range)?,
        }))
    }
}

impl PlacementModifier for OffsetPlacement {
    fn modify(
        &self,
        _context: &PlacementContext<'_>,
        random: &mut dyn RandomSource,
        origin: BlockPos,
        output: &mut Vec<BlockPos>,
    ) {
        let x = self.x.sample(random);
        let y = self.y.sample(random);
        let z = self.z.sample(random);
        output.push(origin.offset(x, y, z));
    }
}

#[linkme::distributed_slice(PLACEMENT_MODIFIER_TYPES)]
static OFFSET: TypeEntry<dyn PlacementModifier> = TypeEntry {
    name: "offset",
    decode: OffsetPlacement::decode,
};
