//! `MatchingBlockTagPredicate` (`matching_block_tag`).

use mc_core::pos::Vec3i;
use mc_data::block::{Block, BlockState};
use mc_data::tags::TagSet;
use mc_game_core::world::level::levelgen::blockpredicates::BLOCK_PREDICATE_TYPES;
use mc_game_core::world::level::levelgen::blockpredicates::state_testing_predicate::{
    StateTestingPredicate, state_testing_codec,
};
use mc_game_core::world::level::levelgen::codec::{TypeEntry, parse_identifier};
use std::sync::Arc;

pub struct MatchingBlockTagPredicate {
    offset: Vec3i,
    /// `None`: a tag that does not exist (vanilla's unbound tag holder set: matches nothing).
    tag: Option<&'static TagSet<Block>>,
}

impl MatchingBlockTagPredicate {
    /// `new MatchingBlockTagPredicate(offset, TagKey)`: `tag` is the tag id (`minecraft:air`).
    pub fn new(offset: Vec3i, tag: &str) -> MatchingBlockTagPredicate {
        MatchingBlockTagPredicate {
            offset,
            tag: mc_data::tags::get::<Block>(tag),
        }
    }
}

impl StateTestingPredicate for MatchingBlockTagPredicate {
    fn offset(&self) -> &Vec3i {
        &self.offset
    }

    fn test_state(&self, state: BlockState) -> bool {
        self.tag.is_some_and(|tag| tag.contains(state.block()))
    }
}

#[linkme::distributed_slice(BLOCK_PREDICATE_TYPES)]
static MATCHING_BLOCK_TAG: TypeEntry<
    dyn mc_game_core::world::level::levelgen::blockpredicates::BlockPredicate,
> = TypeEntry {
    name: "matching_block_tag",
    decode: |fields, _| {
        let offset = state_testing_codec(fields)?;
        // `TagKey.codec(Registries.BLOCK)`: a plain identifier (no leading `#`).
        let tag = fields.required("tag", parse_identifier)?;
        Ok(Arc::new(MatchingBlockTagPredicate {
            offset,
            tag: mc_data::tags::get::<Block>(&tag.to_string()),
        }))
    },
};
