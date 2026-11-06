//! `rootplacers.MangroveRootPlacer`.

use super::ROOT_PLACER_TYPES;
use super::mangrove_root_placement::MangroveRootPlacement;
use super::root_placer::{
    RootBase, RootPlacer, get_potentially_waterlogged_state, place_root_default,
};
use super::tree_feature::{self, TreeFeature, TreeSetter};
use mc_core::direction::{Direction, Plane};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use std::sync::Arc;

pub struct MangroveRootPlacer {
    base: RootBase,
    mangrove_root_placement: MangroveRootPlacement,
}

impl MangroveRootPlacer {
    fn simulate_roots(
        &self,
        level: &dyn LevelAccessor,
        random: &mut dyn RandomSource,
        root_pos: BlockPos,
        dir: Direction,
        root_origin: BlockPos,
        root_positions: &mut Vec<BlockPos>,
        layer: i32,
    ) -> bool {
        let max_root_length = self.mangrove_root_placement.max_root_length;
        if layer != max_root_length && root_positions.len() as i32 <= max_root_length {
            for pos in self.potential_root_positions(root_pos, dir, random, root_origin) {
                if self.can_place_root(level, pos) {
                    root_positions.push(pos);
                    if !self.simulate_roots(
                        level,
                        random,
                        pos,
                        dir,
                        root_origin,
                        root_positions,
                        layer + 1,
                    ) {
                        return false;
                    }
                }
            }
            true
        } else {
            false
        }
    }

    fn potential_root_positions(
        &self,
        pos: BlockPos,
        prev_dir: Direction,
        random: &mut dyn RandomSource,
        root_origin: BlockPos,
    ) -> Vec<BlockPos> {
        let below = pos.below();
        let next_to = pos.relative(prev_dir);
        let width = pos.dist_manhattan(&root_origin);
        let max_root_width = self.mangrove_root_placement.max_root_width;
        let random_skew_chance = self.mangrove_root_placement.random_skew_chance;
        if width > max_root_width - 3 && width <= max_root_width {
            if random.next_float() < random_skew_chance {
                vec![below, next_to.below()]
            } else {
                vec![below]
            }
        } else if width > max_root_width {
            vec![below]
        } else if random.next_float() < random_skew_chance {
            vec![below]
        } else if random.next_boolean() {
            vec![next_to]
        } else {
            vec![below]
        }
    }
}

impl RootPlacer for MangroveRootPlacer {
    fn base(&self) -> &RootBase {
        &self.base
    }

    fn place_roots(
        &self,
        level: &mut dyn WorldGenLevel,
        root_setter: &mut TreeSetter,
        random: &mut dyn RandomSource,
        origin: BlockPos,
        trunk_origin: BlockPos,
        tree: &TreeFeature,
    ) -> bool {
        let mut root_positions = Vec::new();
        let mut column_pos = origin;
        while column_pos.y() < trunk_origin.y() {
            if !self.can_place_root(level, column_pos) {
                return false;
            }
            column_pos = column_pos.relative(Direction::Up);
        }

        root_positions.push(trunk_origin.below());

        for dir in Plane::Horizontal {
            let pos = trunk_origin.relative(dir);
            let mut positions_in_direction = Vec::new();
            if !self.simulate_roots(
                level,
                random,
                pos,
                dir,
                trunk_origin,
                &mut positions_in_direction,
                0,
            ) {
                return false;
            }
            root_positions.extend(positions_in_direction);
            root_positions.push(trunk_origin.relative(dir));
        }

        for root_pos in root_positions {
            self.place_root(level, root_setter, random, root_pos, tree);
        }
        true
    }

    fn can_place_root(&self, level: &dyn LevelAccessor, pos: BlockPos) -> bool {
        tree_feature::valid_tree_pos(level, pos)
            || self
                .mangrove_root_placement
                .can_grow_through
                .contains(level.get_block_state(pos).block())
    }

    fn place_root(
        &self,
        level: &mut dyn WorldGenLevel,
        root_setter: &mut TreeSetter,
        random: &mut dyn RandomSource,
        pos: BlockPos,
        tree: &TreeFeature,
    ) {
        if self
            .mangrove_root_placement
            .muddy_roots_in
            .contains(level.get_block_state(pos).block())
        {
            let muddy_roots = self
                .mangrove_root_placement
                .muddy_roots_provider
                .value()
                .get_state(level, random, pos);
            let state = get_potentially_waterlogged_state(level, pos, muddy_roots);
            root_setter.set(level, pos, state);
        } else {
            place_root_default(self, level, root_setter, random, pos, tree);
        }
    }
}

#[linkme::distributed_slice(ROOT_PLACER_TYPES)]
static MANGROVE_ROOT_PLACER: TypeEntry<dyn RootPlacer> = TypeEntry {
    name: "mangrove_root_placer",
    decode: |fields, ctx| {
        let base = RootBase::decode(fields, ctx)?;
        let mangrove_root_placement = fields.required("mangrove_root_placement", |v| {
            MangroveRootPlacement::decode(v, ctx)
        })?;
        Ok(Arc::new(MangroveRootPlacer {
            base,
            mangrove_root_placement,
        }))
    },
};
