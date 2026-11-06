//! `net.minecraft.world.level.block.MushroomBlock` (extends `VegetationBlock`): brown and red mushrooms.
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::block::bonemeal::BonemealSource;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::vegetation_block::MayPlaceOn;
use mc_game_core::world::level::blockscan;
use mc_game_core::{register_block_class, update_shape};

const SEARCH_RADIUS: i32 = 4;
const MAX_NEIGHBORS_TO_GROW: i32 = 4;

pub struct MushroomBlock {
    /// `feature` (`worldgen/feature` id).
    feature: &'static str,
}

impl MayPlaceOn for MushroomBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_solid_render()
    }
}

impl MushroomBlock {
    /// `canSpread`.
    fn can_spread(&self, level: &ServerLevel, block: Block, pos: BlockPos) -> bool {
        let min_pos = pos.offset(-SEARCH_RADIUS, -1, -SEARCH_RADIUS);
        let max_pos = pos.offset(SEARCH_RADIUS, 1, SEARCH_RADIUS);
        blockscan::at_most_matched(
            level,
            min_pos,
            max_pos,
            &|state| state.is_block(block),
            MAX_NEIGHBORS_TO_GROW,
        )
    }

    /// `growMushroom(level, pos, state, random)`.
    pub fn grow_mushroom(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState) -> bool {
        let Some(feature) = level.lookup_feature(self.feature) else {
            return false;
        };
        level.remove_block(pos, false);
        if level.place_feature_value(&*feature, pos) {
            true
        } else {
            level.set_block_and_update(pos, state);
            false
        }
    }
}

impl BlockBehaviour for MushroomBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below_pos = pos.below();
        let below = level.get_block_state(below_pos);
        if below.is_tag("overrides_mushroom_light_requirement") {
            true
        } else {
            level.get_raw_brightness(pos, 0) < 13 && self.may_place_on(below, level, below_pos)
        }
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            mc_game_core::world::level::block::impls::vegetation_block::update_shape(
                state, &*level, pos,
            )
        }
    );
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, mut pos: BlockPos) {
        if level.random().next_int_bounded(25) == 0 {
            if !self.can_spread(level, state.block(), pos) {
                return;
            }
            let mut offset = random_offset(level, pos);
            for _ in 0..4 {
                if level.is_empty_block(offset) && state.can_survive(&*level, offset) {
                    pos = offset;
                }
                offset = random_offset(level, pos);
            }
            if level.is_empty_block(offset) && state.can_survive(&*level, offset) {
                level.set_block_flags(offset, state, 2);
            }
        }
    }
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(
        &self,
        _level: &dyn LevelReader,
        _pos: BlockPos,
        _state: BlockState,
        _source: BonemealSource,
    ) -> bool {
        false
    }
    fn is_valid_bonemeal_target_server(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        _state: BlockState,
        _source: BonemealSource,
    ) -> bool {
        match level
            .lookup_feature(self.feature)
            .and_then(|feature| feature.huge_mushroom_foliage_radius())
        {
            Some(foliage_radius) => {
                !level.is_outside_build_height(pos.above_n(4 + foliage_radius).y())
            }
            None => false,
        }
    }
    fn is_bonemeal_success(
        &self,
        level: &mut ServerLevel,
        _pos: BlockPos,
        _state: BlockState,
        _source: BonemealSource,
    ) -> bool {
        level.random().next_float() < 0.4
    }
    fn perform_bonemeal(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        state: BlockState,
        _source: BonemealSource,
    ) {
        self.grow_mushroom(level, pos, state);
    }
}

/// `pos.offset(random.nextInt(3) - 1, random.nextInt(2) - random.nextInt(2), random.nextInt(3) - 1)`.
fn random_offset(level: &mut ServerLevel, pos: BlockPos) -> BlockPos {
    let random = level.random();
    pos.offset(
        random.next_int_bounded(3) - 1,
        random.next_int_bounded(2) - random.next_int_bounded(2),
        random.next_int_bounded(3) - 1,
    )
}

register_block_class!("MushroomBlock", |block| Box::new(MushroomBlock {
    feature: match block.data().name {
        "minecraft:brown_mushroom" => "huge_brown_mushroom",
        "minecraft:red_mushroom" => "huge_red_mushroom",
        name => panic!("unexpected MushroomBlock {name}"),
    },
}));
