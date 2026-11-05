//! `HeightRangePredicate` (`height_range`).

use mc_game_core::world::level::levelgen::blockpredicates::{BLOCK_PREDICATE_TYPES, BlockPredicate};
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_core::pos::BlockPos;
use mc_worldgen::vertical_anchor::VerticalAnchor;
use mc_worldgen::world_generation_context::WorldGenerationContext;
use std::sync::Arc;

pub struct HeightRangePredicate {
    pub min_inclusive: VerticalAnchor,
    pub max_inclusive: VerticalAnchor,
}

impl BlockPredicate for HeightRangePredicate {
    fn test(&self, level: &dyn LevelAccessor, pos: BlockPos) -> bool {
        // `WorldGenerationContext.of(level)`
        let context = match level.as_world_gen_level() {
            Some(world_gen_level) => world_gen_level.env().world_generation_context(level.level_height()),
            None => WorldGenerationContext::of_level(level.get_min_y(), level.get_height(), level.get_sea_level()),
        };
        let min = self.min_inclusive.resolve_y(&context);
        let max = self.max_inclusive.resolve_y(&context);
        pos.y() >= min && pos.y() <= max
    }
}

#[linkme::distributed_slice(BLOCK_PREDICATE_TYPES)]
static HEIGHT_RANGE: TypeEntry<dyn BlockPredicate> = TypeEntry {
    name: "height_range",
    decode: |fields, _| {
        let min_inclusive = fields.required("min_inclusive", VerticalAnchor::from_json)?;
        let max_inclusive = fields.required("max_inclusive", VerticalAnchor::from_json)?;
        Ok(Arc::new(HeightRangePredicate { min_inclusive, max_inclusive }))
    },
};
