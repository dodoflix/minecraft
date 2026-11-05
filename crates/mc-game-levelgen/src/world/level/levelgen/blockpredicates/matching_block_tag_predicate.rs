//! `MatchingBlockTagPredicate` (`matching_block_tag`).

use super::BLOCK_PREDICATE_TYPES;
use super::state_testing_predicate::{StateTestingPredicate, state_testing_codec};
use crate::world::level::levelgen::codec::{TypeEntry, parse_identifier};
use mc_core::pos::Vec3i;
use mc_data::block::{Block, BlockState};
use mc_data::tags::TagSet;
use std::sync::Arc;

pub struct MatchingBlockTagPredicate {
    offset: Vec3i,
    /// `None`: a tag that does not exist (vanilla's unbound tag holder set: matches nothing).
    tag: Option<&'static TagSet<Block>>,
}

impl MatchingBlockTagPredicate {
    /// `new MatchingBlockTagPredicate(offset, TagKey)`: `tag` is the tag id (`minecraft:air`).
    pub fn new(offset: Vec3i, tag: &str) -> MatchingBlockTagPredicate {
        MatchingBlockTagPredicate { offset, tag: mc_data::tags::get::<Block>(tag) }
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
static MATCHING_BLOCK_TAG: TypeEntry<dyn super::BlockPredicate> = TypeEntry {
    name: "matching_block_tag",
    decode: |fields, _| {
        let offset = state_testing_codec(fields)?;
        // `TagKey.codec(Registries.BLOCK)`: a plain identifier (no leading `#`).
        let tag = fields.required("tag", |v| parse_identifier(v))?;
        Ok(Arc::new(MatchingBlockTagPredicate { offset, tag: mc_data::tags::get::<Block>(&tag.to_string()) }))
    },
};
