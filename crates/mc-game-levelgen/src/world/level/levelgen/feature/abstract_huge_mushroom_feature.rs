//! `AbstractHugeMushroomFeature`: the interface (with default methods) shared by the huge brown and red mushrooms.

use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::levelgen::blockpredicates::BlockPredicate;
use mc_game_core::world::level::levelgen::holder::Holder;
use mc_game_core::world::level::levelgen::stateproviders::BlockStateProvider;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;

pub const MIN_MUSHROOM_HEIGHT: i32 = 4;

pub trait AbstractHugeMushroomFeature {
    fn cap_provider(&self) -> &Holder<dyn BlockStateProvider>;
    fn stem_provider(&self) -> &Holder<dyn BlockStateProvider>;
    fn foliage_radius(&self) -> i32;
    fn can_place_on(&self) -> &dyn BlockPredicate;

    fn get_tree_radius_for_height(
        &self,
        trunk_height: i32,
        tree_height: i32,
        leaf_radius: i32,
        yo: i32,
    ) -> i32;

    fn make_cap(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        origin: BlockPos,
        tree_height: i32,
    );

    fn place_trunk(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        origin: BlockPos,
        tree_height: i32,
    ) {
        for dy in 0..tree_height {
            let block_pos = origin.relative_n(Direction::Up, dy);
            let state = self
                .stem_provider()
                .value()
                .get_state(&*level, random, origin);
            self.place_mushroom_block(level, block_pos, state);
        }
    }

    fn place_mushroom_block(
        &self,
        level: &mut dyn LevelAccessor,
        block_pos: BlockPos,
        new_state: BlockState,
    ) {
        let current_state = level.get_block_state(block_pos);
        if current_state.is_air() || current_state.is_tag("replaceable_by_mushrooms") {
            level.set_block_and_update(block_pos, new_state);
        }
    }

    fn get_tree_height(&self, random: &mut dyn RandomSource) -> i32 {
        let mut tree_height = random.next_int_bounded(3) + 4;
        if random.next_int_bounded(12) == 0 {
            tree_height *= 2;
        }
        tree_height
    }

    fn is_valid_position(
        &self,
        level: &dyn WorldGenLevel,
        origin: BlockPos,
        tree_height: i32,
    ) -> bool {
        let y = origin.y();
        if y >= level.get_min_y() + 1 && y + tree_height + 1 <= level.get_max_y() {
            if !self.can_place_on().test(level, origin.below()) {
                return false;
            }
            for dy in 0..=tree_height {
                let radius = self.get_tree_radius_for_height(-1, -1, self.foliage_radius(), dy);
                for dx in -radius..=radius {
                    for dz in -radius..=radius {
                        let state = level.get_block_state(origin.offset(dx, dy, dz));
                        if !state.is_air() && !state.is_tag("leaves") {
                            return false;
                        }
                    }
                }
            }
            true
        } else {
            false
        }
    }
}

/// `AbstractHugeMushroomFeature.place`.
pub fn place<T: AbstractHugeMushroomFeature + ?Sized>(
    this: &T,
    level: &mut dyn WorldGenLevel,
    random: &mut dyn RandomSource,
    origin: BlockPos,
) -> bool {
    let tree_height = this.get_tree_height(random);
    if !this.is_valid_position(&*level, origin, tree_height) {
        return false;
    }
    this.make_cap(level, random, origin, tree_height);
    this.place_trunk(level, random, origin, tree_height);
    true
}
