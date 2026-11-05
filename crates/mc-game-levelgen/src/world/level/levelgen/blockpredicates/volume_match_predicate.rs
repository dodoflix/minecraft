//! `VolumeMatchPredicate` (`volume_match`).

use mc_game_core::world::level::levelgen::blockpredicates::{BLOCK_PREDICATE_TYPES, BlockPredicate, decode};
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::levelgen::codec::{LoadError, TypeEntry, parse_vec3i_offset};
use mc_core::pos::{BlockPos, Vec3i};
use std::sync::Arc;

pub struct VolumeMatchPredicate {
    pub min: Vec3i,
    pub max: Vec3i,
    pub match_: Arc<dyn BlockPredicate>,
}

impl BlockPredicate for VolumeMatchPredicate {
    fn test(&self, level: &dyn LevelAccessor, block_pos: BlockPos) -> bool {
        for ox in self.min.x..=self.max.x {
            for oz in self.min.z..=self.max.z {
                for oy in self.min.y..=self.max.y {
                    if !self.match_.test(level, block_pos.offset(ox, oy, oz)) {
                        return false;
                    }
                }
            }
        }
        true
    }
}

#[linkme::distributed_slice(BLOCK_PREDICATE_TYPES)]
static VOLUME_MATCH: TypeEntry<dyn BlockPredicate> = TypeEntry {
    name: "volume_match",
    decode: |fields, ctx| {
        let min = fields.required("min", |v| parse_vec3i_offset(v, 16))?;
        let max = fields.required("max", |v| parse_vec3i_offset(v, 16))?;
        let match_ = fields.required("match", |v| decode(v, ctx))?;
        if !(min.x <= max.x && min.y <= max.y && min.z <= max.z) {
            return Err(LoadError::new("min bound cannot be larger than max bound"));
        }
        Ok(Arc::new(VolumeMatchPredicate { min, max, match_ }))
    },
};
