//! `HasSturdyFacePredicate` (`has_sturdy_face`).

use mc_game_core::world::level::levelgen::blockpredicates::{BLOCK_PREDICATE_TYPES, BlockPredicate};
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::levelgen::codec::{TypeEntry, parse_direction, parse_vec3i_offset};
use mc_game_core::world::level::LevelAccessor;
use mc_core::direction::Direction;
use mc_core::pos::{BlockPos, Vec3i};
use std::sync::Arc;

pub struct HasSturdyFacePredicate {
    pub offset: Vec3i,
    pub direction: Direction,
}

impl BlockPredicate for HasSturdyFacePredicate {
    fn test(&self, level: &dyn LevelAccessor, origin: BlockPos) -> bool {
        let test_position = origin.offset_vec(&self.offset);
        level.get_block_state(test_position).is_face_sturdy_at(level, test_position, self.direction)
    }
}

#[linkme::distributed_slice(BLOCK_PREDICATE_TYPES)]
static HAS_STURDY_FACE: TypeEntry<dyn BlockPredicate> = TypeEntry {
    name: "has_sturdy_face",
    decode: |fields, _| {
        let offset = fields.optional("offset", |v| parse_vec3i_offset(v, 16))?.unwrap_or(Vec3i::ZERO);
        let direction = fields.required("direction", parse_direction)?;
        Ok(Arc::new(HasSturdyFacePredicate { offset, direction }))
    },
};
