//! `CombiningPredicate`: the shared `predicates` list codec of `all_of` / `any_of`.

use super::{BlockPredicate, decode};
use crate::world::level::levelgen::codec::{Fields, LoadResult, parse_list};
use crate::world::level::levelgen::registries::DecodeCtx;
use std::sync::Arc;

/// `BlockPredicate.CODEC.listOf().fieldOf("predicates")`.
pub fn predicates_codec(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Vec<Arc<dyn BlockPredicate>>> {
    fields.required("predicates", |v| parse_list(v, |p| decode(p, ctx)))
}
