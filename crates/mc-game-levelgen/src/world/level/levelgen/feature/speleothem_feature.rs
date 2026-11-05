//! `SpeleothemFeature` (`speleothem`): a single pointed dripstone / sulfur spike with a patch of base blocks behind it.

use super::speleothem_utils as utils;
use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature};
use mc_game_core::world::level::levelgen::GenEnv;
use mc_game_core::world::level::levelgen::WorldGenLevel;
use mc_game_core::world::level::levelgen::codec::{BlockSet, Fields, LoadResult, TypeEntry, parse_block_state, parse_float_range};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::{BlockGetter, LevelAccessor};
use mc_core::direction::{Direction, Plane};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use std::sync::Arc;

pub struct SpeleothemFeature {
    pub base_block: BlockState,
    pub pointed_block: BlockState,
    pub replaceable_blocks: BlockSet,
    pub chance_of_taller_generation: f32,
    pub chance_of_directional_spread: f32,
    pub chance_of_spread_radius2: f32,
    pub chance_of_spread_radius3: f32,
}

impl SpeleothemFeature {
    pub fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        let unit = |v: &serde_json::Value| parse_float_range(v, 0.0, 1.0);
        Ok(Arc::new(SpeleothemFeature {
            base_block: fields.required("base_block", parse_block_state)?,
            pointed_block: fields.required("pointed_block", parse_block_state)?,
            replaceable_blocks: fields.required("replaceable_blocks", BlockSet::from_json)?,
            chance_of_taller_generation: fields.optional("chance_of_taller_generation", unit)?.unwrap_or(0.2),
            chance_of_directional_spread: fields.optional("chance_of_directional_spread", unit)?.unwrap_or(0.7),
            chance_of_spread_radius2: fields.optional("chance_of_spread_radius2", unit)?.unwrap_or(0.5),
            chance_of_spread_radius3: fields.optional("chance_of_spread_radius3", unit)?.unwrap_or(0.5),
        }))
    }

    /// `getTipDirection(level, pos, random)`.
    fn get_tip_direction(&self, level: &dyn BlockGetter, pos: BlockPos, random: &mut dyn RandomSource) -> Option<Direction> {
        let base = self.base_block.block();
        let can_place_above = utils::is_base(level.get_block_state(pos.above()), base, &self.replaceable_blocks);
        let can_place_below = utils::is_base(level.get_block_state(pos.below()), base, &self.replaceable_blocks);
        if can_place_above && can_place_below {
            Some(if random.next_boolean() { Direction::Down } else { Direction::Up })
        } else if can_place_above {
            Some(Direction::Down)
        } else if can_place_below {
            Some(Direction::Up)
        } else {
            None
        }
    }

    /// `createPatchOfBaseBlocks(level, random, pos)`.
    fn create_patch_of_base_blocks(&self, level: &mut dyn LevelAccessor, random: &mut dyn RandomSource, pos: BlockPos) {
        let base: Block = self.base_block.block();
        utils::place_base_block_if_possible(level, pos, base, &self.replaceable_blocks);
        for direction in Plane::Horizontal {
            if !(random.next_float() > self.chance_of_directional_spread) {
                let pos1 = pos.relative(direction);
                utils::place_base_block_if_possible(level, pos1, base, &self.replaceable_blocks);
                if !(random.next_float() > self.chance_of_spread_radius2) {
                    let pos2 = pos1.relative(Direction::get_random(random));
                    utils::place_base_block_if_possible(level, pos2, base, &self.replaceable_blocks);
                    if !(random.next_float() > self.chance_of_spread_radius3) {
                        let pos3 = pos2.relative(Direction::get_random(random));
                        utils::place_base_block_if_possible(level, pos3, base, &self.replaceable_blocks);
                    }
                }
            }
        }
    }
}

impl Feature for SpeleothemFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let Some(tip_direction) = self.get_tip_direction(level, origin, random) else {
            return false;
        };
        let root_pos = origin.relative(tip_direction.get_opposite());
        self.create_patch_of_base_blocks(level, random, root_pos);
        let height = if random.next_float() < self.chance_of_taller_generation
            && utils::is_empty_or_water(level.get_block_state(origin.relative(tip_direction)))
        {
            2
        } else {
            1
        };
        utils::grow_speleothem(level, origin, tip_direction, height, false, self.base_block.block(), self.pointed_block.block(), &self.replaceable_blocks);
        true
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static SPELEOTHEM: TypeEntry<dyn Feature> = TypeEntry { name: "speleothem", decode: SpeleothemFeature::decode };
