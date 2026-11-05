//! `MultifaceGrowthFeature` (`multiface_growth`): glow lichen / sculk vein patches.

use super::tree::shuffle;
use super::{FEATURE_TYPES, Feature};
use crate::world::level::block::BlockStateExt;
use crate::world::level::block::impls::multiface_block;
use crate::world::level::block::impls::multiface_spreadeable_block::{get_spreader, is_multiface_spreadeable};
use crate::world::level::levelgen::codec::{BlockSet, Fields, LoadError, LoadResult, TypeEntry, parse_block, parse_bool, parse_float_range, parse_int_range};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use std::sync::Arc;

pub struct MultifaceGrowthFeature {
    pub place_block: Block,
    pub search_range: i32,
    pub can_place_on_floor: bool,
    pub can_place_on_ceiling: bool,
    pub can_place_on_wall: bool,
    pub chance_of_spreading: f32,
    pub can_be_placed_on: BlockSet,
}

impl MultifaceGrowthFeature {
    fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        let place_block = fields.required("block", |v| {
            let block = parse_block(v)?;
            if is_multiface_spreadeable(block) { Ok(block) } else { Err(LoadError::new("Growth block should be a multiface spreadeable block")) }
        })?;
        Ok(Arc::new(MultifaceGrowthFeature {
            place_block,
            search_range: fields.optional("search_range", |v| parse_int_range(v, 1, 64))?.unwrap_or(10),
            can_place_on_floor: fields.optional("can_place_on_floor", parse_bool)?.unwrap_or(false),
            can_place_on_ceiling: fields.optional("can_place_on_ceiling", parse_bool)?.unwrap_or(false),
            can_place_on_wall: fields.optional("can_place_on_wall", parse_bool)?.unwrap_or(false),
            chance_of_spreading: fields.optional("chance_of_spreading", |v| parse_float_range(v, 0.0, 1.0))?.unwrap_or(0.5),
            can_be_placed_on: fields.required("can_be_placed_on", BlockSet::from_json)?,
        }))
    }

    /// `validDirections()`.
    fn valid_directions(&self) -> Vec<Direction> {
        let mut valid = Vec::with_capacity(6);
        if self.can_place_on_ceiling {
            valid.push(Direction::Up);
        }
        if self.can_place_on_floor {
            valid.push(Direction::Down);
        }
        if self.can_place_on_wall {
            valid.extend(Direction::HORIZONTAL);
        }
        valid
    }

    /// `getShuffledDirectionsExcept(random, excludeDirection)`.
    fn get_shuffled_directions_except(&self, random: &mut dyn RandomSource, exclude: Direction) -> Vec<Direction> {
        let mut directions: Vec<Direction> = self.valid_directions().into_iter().filter(|d| *d != exclude).collect();
        shuffle(&mut directions, random);
        directions
    }

    /// `getShuffledDirections(random)`.
    fn get_shuffled_directions(&self, random: &mut dyn RandomSource) -> Vec<Direction> {
        let mut directions = self.valid_directions();
        shuffle(&mut directions, random);
        directions
    }

    /// `placeGrowthIfPossible(placerBlock, level, pos, oldState, random, placementDirections)`.
    pub fn place_growth_if_possible(
        &self,
        level: &mut dyn WorldGenLevel,
        pos: BlockPos,
        old_state: BlockState,
        random: &mut dyn RandomSource,
        placement_directions: &[Direction],
    ) -> bool {
        for &placement_direction in placement_directions {
            let neighbour_state = level.get_block_state(pos.relative(placement_direction));
            if self.can_be_placed_on.contains(neighbour_state.block()) {
                let Some(new_state) = multiface_block::get_state_for_placement(self.place_block, old_state, &*level, pos, placement_direction) else {
                    return false;
                };
                level.set_block_and_update(pos, new_state);
                level.mark_pos_for_post_processing(pos);
                if random.next_float() < self.chance_of_spreading {
                    // `placeBlock instanceof MultifaceSpreadeableBlock` is checked by the codec
                    if let Some(spreader) = get_spreader(self.place_block) {
                        spreader.spread_from_face_toward_random_direction(new_state, level, pos, placement_direction, random, true);
                    }
                }
                return true;
            }
        }
        false
    }
}

/// `isAirOrWater(state)`.
fn is_air_or_water(state: BlockState) -> bool {
    state.is_air() || state.is_block(Block::WATER)
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static MULTIFACE_GROWTH: TypeEntry<dyn Feature> = TypeEntry { name: "multiface_growth", decode: MultifaceGrowthFeature::decode };

impl Feature for MultifaceGrowthFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        if !is_air_or_water(level.get_block_state(origin)) {
            return false;
        }
        let directions = self.get_shuffled_directions(random);
        let origin_state = level.get_block_state(origin);
        if self.place_growth_if_possible(level, origin, origin_state, random, &directions) {
            return true;
        }
        for &search_direction in &directions {
            let placement_directions = self.get_shuffled_directions_except(random, search_direction.get_opposite());
            for _ in 0..self.search_range {
                // vanilla offsets from `origin` on every iteration (the search never moves further than one block)
                let pos = origin.relative(search_direction);
                let state = level.get_block_state(pos);
                if !is_air_or_water(state) && !state.is_block(self.place_block) {
                    break;
                }
                if self.place_growth_if_possible(level, pos, state, random, &placement_directions) {
                    return true;
                }
            }
        }
        false
    }
}
