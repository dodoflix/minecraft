//! `MatchingBiomesPredicate` (`matching_biomes`).

use mc_core::pos::BlockPos;
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::levelgen::blockpredicates::{
    BLOCK_PREDICATE_TYPES, BlockPredicate,
};
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_world::palette::BiomeId;
use std::sync::Arc;

pub struct MatchingBiomesPredicate {
    /// `HolderSet<Biome>` resolved to biome ids at decode time.
    pub biomes: Vec<BiomeId>,
}

impl BlockPredicate for MatchingBiomesPredicate {
    fn test(&self, level: &dyn LevelAccessor, pos: BlockPos) -> bool {
        let level = level
            .as_world_gen_level()
            .expect("matching_biomes needs a level with generation biome access (WorldGenLevel)");
        self.biomes.contains(&level.get_biome(pos))
    }
}

#[linkme::distributed_slice(BLOCK_PREDICATE_TYPES)]
static MATCHING_BIOMES: TypeEntry<dyn BlockPredicate> = TypeEntry {
    name: "matching_biomes",
    decode: |fields, ctx| {
        Ok(Arc::new(MatchingBiomesPredicate {
            biomes: fields.required("biomes", |v| ctx.biomes.holder_set_from_json(v))?,
        }))
    },
};
