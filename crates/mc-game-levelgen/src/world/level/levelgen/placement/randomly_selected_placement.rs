//! `RandomlySelectedPlacement`: delegates to one uniformly chosen modifier of the list.

use mc_game_core::world::level::levelgen::placement::{PLACEMENT_MODIFIER_TYPES, PlacementContext, PlacementModifier, decode_modifier};
use mc_game_core::world::level::levelgen::codec::{Fields, LoadError, LoadResult, TypeEntry, parse_list};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct RandomlySelectedPlacement {
    pub placements: Vec<Arc<dyn PlacementModifier>>,
}

impl RandomlySelectedPlacement {
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PlacementModifier>> {
        let placements = fields.required("placements", |v| {
            let list = parse_list(v, |m| decode_modifier(m, ctx))?;
            if list.is_empty() { Err(LoadError::new("List must have contents")) } else { Ok(list) }
        })?;
        Ok(Arc::new(RandomlySelectedPlacement { placements }))
    }
}

impl PlacementModifier for RandomlySelectedPlacement {
    fn modify(&self, context: &PlacementContext<'_>, random: &mut dyn RandomSource, origin: BlockPos, output: &mut Vec<BlockPos>) {
        let index = random.next_int_bounded(self.placements.len() as i32) as usize;
        self.placements[index].modify(context, random, origin, output);
    }
}

#[linkme::distributed_slice(PLACEMENT_MODIFIER_TYPES)]
static RANDOMLY_SELECTED: TypeEntry<dyn PlacementModifier> =
    TypeEntry { name: "randomly_selected", decode: RandomlySelectedPlacement::decode };
