//! `AbstractOreFeature`: the `targets` / `size` / `discard_chance_on_air_exposure` record shared by `ore` and `scattered_ore`
//! plus the placement test (`canPlaceOre`).

use super::block_replacement::BlockReplacement;
use crate::world::level::levelgen::codec::{Fields, LoadResult, parse_float_range, parse_int_range, parse_list};
use crate::world::level::levelgen::registries::DecodeCtx;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;

pub struct AbstractOreFeature {
    pub target_states: Vec<BlockReplacement>,
    pub size: i32,
    pub discard_chance_on_air_exposure: f32,
}

impl AbstractOreFeature {
    /// `AbstractOreFeature.makeCodec`.
    pub fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<AbstractOreFeature> {
        Ok(AbstractOreFeature {
            target_states: fields.required("targets", |v| parse_list(v, |e| BlockReplacement::decode(e, ctx)))?,
            size: fields.required("size", |v| parse_int_range(v, 0, 64))?,
            discard_chance_on_air_exposure: fields.required("discard_chance_on_air_exposure", |v| parse_float_range(v, 0.0, 1.0))?,
        })
    }

    /// `canPlaceOre(state, blockGetter, random, targetState, orePos)`.
    pub fn can_place_ore(
        &self,
        state: BlockState,
        block_getter: &dyn Fn(BlockPos) -> BlockState,
        random: &mut dyn RandomSource,
        target_state: &BlockReplacement,
        ore_pos: BlockPos,
    ) -> bool {
        if !target_state.target.test(state, ore_pos, random) {
            return false;
        }
        should_skip_air_check(random, self.discard_chance_on_air_exposure) || !is_adjacent_to_air(block_getter, ore_pos)
    }
}

/// `isAdjacentToAir(blockGetter, pos)`.
pub fn is_adjacent_to_air(block_getter: &dyn Fn(BlockPos) -> BlockState, pos: BlockPos) -> bool {
    check_neighbors(block_getter, pos, |state| state.is_air())
}

/// `checkNeighbors(blockGetter, pos, predicate)`.
pub fn check_neighbors(block_getter: &dyn Fn(BlockPos) -> BlockState, pos: BlockPos, predicate: impl Fn(BlockState) -> bool) -> bool {
    Direction::ALL.into_iter().any(|direction| predicate(block_getter(pos.relative(direction))))
}

fn should_skip_air_check(random: &mut dyn RandomSource, discard_chance_on_air_exposure: f32) -> bool {
    if discard_chance_on_air_exposure <= 0.0 {
        true
    } else if discard_chance_on_air_exposure >= 1.0 {
        false
    } else {
        random.next_float() >= discard_chance_on_air_exposure
    }
}
