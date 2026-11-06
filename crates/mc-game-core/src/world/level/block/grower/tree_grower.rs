//! `TreeGrower`: which tree feature a sapling grows into (`worldgen/feature` ids of `TreeFeatures`).
use crate::server::level::ServerLevel;
use crate::world::level::block::BlockStateExt;
use crate::world::level::blockscan;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;

/// `WeightedList<ResourceKey<Feature>>`: `(feature id, weight)`.
type Trees = &'static [(&'static str, i32)];

pub struct TreeGrower {
    pub name: &'static str,
    trees: Trees,
    mega_trees: Trees,
    flower_trees: Trees,
    shortest_tree_type: Option<&'static str>,
}

/// `WeightedList.getRandom(random)`: `nextInt(totalWeight)` (even for a single entry), none for an empty list.
fn get_random(list: Trees, random: &mut dyn RandomSource) -> Option<&'static str> {
    let total_weight: i32 = list.iter().map(|(_, weight)| weight).sum();
    if total_weight == 0 {
        return None;
    }
    let mut selection = random.next_int_bounded(total_weight);
    for (key, weight) in list {
        selection -= weight;
        if selection < 0 {
            return Some(key);
        }
    }
    unreachable!("{selection} exceeded total weight")
}

/// `TreeGrower.TwoByTwoSaplingPos`.
struct TwoByTwoSaplingPos {
    offset_x: i32,
    offset_z: i32,
    ground_level_surrounding_blocks: [(BlockState, BlockPos); 4],
}

macro_rules! grower {
    ($name:literal, $trees:expr, $mega:expr, $flowers:expr, $shortest:expr) => {
        TreeGrower {
            name: $name,
            trees: $trees,
            mega_trees: $mega,
            flower_trees: $flowers,
            shortest_tree_type: $shortest,
        }
    };
}

impl TreeGrower {
    pub const OAK: TreeGrower = grower!(
        "oak",
        &[("oak", 9), ("fancy_oak", 1)],
        &[],
        &[("oak_bees_005", 9), ("fancy_oak_bees_005", 1)],
        Some("oak")
    );
    pub const SPRUCE: TreeGrower = grower!(
        "spruce",
        &[("spruce", 1)],
        &[("mega_spruce", 1), ("mega_pine", 1)],
        &[],
        Some("spruce")
    );
    pub const MANGROVE: TreeGrower = grower!(
        "mangrove",
        &[("mangrove", 15), ("tall_mangrove", 85)],
        &[],
        &[],
        Some("mangrove")
    );
    pub const AZALEA: TreeGrower = grower!(
        "azalea",
        &[("azalea_tree", 1)],
        &[],
        &[],
        Some("azalea_tree")
    );
    pub const BIRCH: TreeGrower = grower!(
        "birch",
        &[("birch", 1)],
        &[],
        &[("birch_bees_005", 1)],
        Some("birch")
    );
    pub const JUNGLE: TreeGrower = grower!(
        "jungle",
        &[("jungle_tree_no_vine", 1)],
        &[("mega_jungle_tree", 1)],
        &[],
        Some("jungle_tree_no_vine")
    );
    pub const ACACIA: TreeGrower = grower!("acacia", &[("acacia", 1)], &[], &[], Some("acacia"));
    pub const CHERRY: TreeGrower = grower!(
        "cherry",
        &[("cherry", 1)],
        &[],
        &[("cherry_bees_005", 1)],
        Some("cherry")
    );
    pub const DARK_OAK: TreeGrower = grower!("dark_oak", &[], &[("dark_oak", 1)], &[], None);
    pub const PALE_OAK: TreeGrower =
        grower!("pale_oak", &[], &[("pale_oak_bonemeal", 1)], &[], None);
    pub const POPLAR: TreeGrower = grower!(
        "poplar",
        &[
            ("red_poplar", 1),
            ("orange_poplar", 1),
            ("yellow_poplar", 1)
        ],
        &[],
        &[],
        Some("red_poplar")
    );

    /// `getConfiguredFeature(random, hasFlowers)`.
    fn get_configured_feature(
        &self,
        random: &mut dyn RandomSource,
        has_flowers: bool,
    ) -> Option<&'static str> {
        if has_flowers && !self.flower_trees.is_empty() {
            get_random(self.flower_trees, random)
        } else {
            get_random(self.trees, random)
        }
    }

    /// `getConfiguredMegaFeature(random)`.
    fn get_configured_mega_feature(&self, random: &mut dyn RandomSource) -> Option<&'static str> {
        get_random(self.mega_trees, random)
    }

    /// `growTree(level, generator, pos, state, random)` (`random` is the level's).
    pub fn grow_tree(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState) -> bool {
        if let Some(mega_feature_key) = self.get_configured_mega_feature(level.random())
            && let Some(feature) = level.lookup_feature(mega_feature_key)
            && let Some(two_by_two) = find_two_by_two_sapling_pos(level, state, pos)
        {
            let blocks = two_by_two.ground_level_surrounding_blocks;
            remove_saplings(level, &blocks);
            if level.place_feature_value(
                &*feature,
                pos.offset(two_by_two.offset_x, 0, two_by_two.offset_z),
            ) {
                return true;
            }
            reset_saplings(level, &blocks);
            return false;
        }
        let has_flowers = self.has_flowers(&*level, pos);
        let Some(feature_key) = self.get_configured_feature(level.random(), has_flowers) else {
            return false;
        };
        let Some(feature) = level.lookup_feature(feature_key) else {
            return false;
        };
        remove_sapling(level, pos);
        if level.place_feature_value(&*feature, pos) {
            true
        } else {
            reset_saplings(level, &[(state, pos)]);
            false
        }
    }

    /// `hasFlowers(level, pos)`.
    fn has_flowers(&self, level: &dyn crate::world::level::LevelReader, pos: BlockPos) -> bool {
        blockscan::any_matched(
            level,
            pos.offset(-2, -1, -2),
            pos.offset(2, 1, 2),
            &|state| state.is_tag("flowers"),
        )
    }

    /// `getMinimumHeight(level)`.
    pub fn get_minimum_height(&self, level: &ServerLevel) -> Option<i32> {
        let key = self.shortest_tree_type?;
        level.lookup_feature(key)?.tree_trunk_base_height()
    }

    /// `canGrow(level, pos, state)` (draws from the level random like vanilla).
    pub fn can_grow(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState) -> bool {
        let has_flowers = self.has_flowers(&*level, pos);
        let feature_key = self.get_configured_feature(level.random(), has_flowers);
        let mega_feature_key = self.get_configured_mega_feature(level.random());
        if feature_key.is_none() && mega_feature_key.is_some() {
            find_two_by_two_sapling_pos(level, state, pos).is_some()
        } else {
            true
        }
    }
}

/// `getSurroundingBlockStates(level, pos, dx, dz)`.
fn get_surrounding_block_states(
    level: &dyn BlockGetter,
    pos: BlockPos,
    dx: i32,
    dz: i32,
) -> [(BlockState, BlockPos); 4] {
    [(0, 0), (1, 0), (0, 1), (1, 1)].map(|(ox, oz)| {
        let p = pos.offset(dx + ox, 0, dz + oz);
        (level.get_block_state(p), p)
    })
}

/// `findTwoByTwoSaplingPos(level, state, pos)`.
fn find_two_by_two_sapling_pos(
    level: &dyn BlockGetter,
    state: BlockState,
    pos: BlockPos,
) -> Option<TwoByTwoSaplingPos> {
    for dx in [0, -1] {
        for dz in [0, -1] {
            let surrounding = get_surrounding_block_states(level, pos, dx, dz);
            if surrounding.iter().all(|(s, _)| s.is_block(state.block())) {
                return Some(TwoByTwoSaplingPos {
                    offset_x: dx,
                    offset_z: dz,
                    ground_level_surrounding_blocks: surrounding,
                });
            }
        }
    }
    None
}

/// `removeSapling(level, pos)`: the fluid of the position replaces it (flags 818).
fn remove_sapling(level: &mut ServerLevel, sapling_pos: BlockPos) {
    let empty_block = level.get_fluid_state(sapling_pos).create_legacy_block();
    level.set_block_flags(sapling_pos, empty_block, 818);
}

fn remove_saplings(level: &mut ServerLevel, sapling_blocks: &[(BlockState, BlockPos)]) {
    for (_, pos) in sapling_blocks {
        remove_sapling(level, *pos);
    }
}

/// `resetSaplings(level, saplingBlocks)` (flags 260).
fn reset_saplings(level: &mut ServerLevel, sapling_blocks: &[(BlockState, BlockPos)]) {
    for (state, pos) in sapling_blocks {
        level.set_block_flags(*pos, *state, 260);
    }
}
