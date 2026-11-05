//! `TagMatchTest` (`tag_match`).

use super::rule_test::{RULE_TEST_TYPES, RuleTest};
use mc_game_core::world::level::levelgen::codec::*;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use std::sync::Arc;

pub struct TagMatchTest {
    pub tag: mc_core::ident::Identifier,
    /// Resolved at decode time; a tag missing from the data pack matches nothing.
    set: Option<&'static mc_data::tags::TagSet<mc_data::block::Block>>,
}

impl TagMatchTest {
    pub fn decode(f: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn RuleTest>> {
        let tag = f.required("tag", parse_identifier)?;
        let set = mc_data::tags::get::<mc_data::block::Block>(&tag.to_string());
        Ok(Arc::new(TagMatchTest { tag, set }))
    }
}

impl RuleTest for TagMatchTest {
    fn test(&self, state: BlockState, _pos: BlockPos, _random: &mut dyn RandomSource) -> bool {
        self.set.is_some_and(|set| set.contains(state.block()))
    }
}

#[linkme::distributed_slice(RULE_TEST_TYPES)]
static TYPE: TypeEntry<dyn RuleTest> = TypeEntry { name: "tag_match", decode: TagMatchTest::decode };
