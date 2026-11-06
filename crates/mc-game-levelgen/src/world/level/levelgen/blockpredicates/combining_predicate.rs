//! `CombiningPredicate`: the shared `predicates` list codec of `all_of` / `any_of`.

use mc_game_core::world::level::levelgen::blockpredicates::{BlockPredicate, decode};
use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, parse_list};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use std::sync::Arc;

/// `BlockPredicate.CODEC.listOf().fieldOf("predicates")`.
pub fn predicates_codec(
    fields: &Fields<'_>,
    ctx: &DecodeCtx<'_>,
) -> LoadResult<Vec<Arc<dyn BlockPredicate>>> {
    fields.required("predicates", |v| parse_list(v, |p| decode(p, ctx)))
}
