//! `RootSystemFeature`.

use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_game_core::world::level::LevelReader;
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::levelgen::blockpredicates::{self, BlockPredicate};
use mc_game_core::world::level::levelgen::codec::{
    BlockSet, Fields, LoadResult, TypeEntry, parse_int_range,
};
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature};
use mc_game_core::world::level::levelgen::holder::Holder;
use mc_game_core::world::level::levelgen::placement::PlacedFeature;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::levelgen::stateproviders::BlockStateProvider;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_game_core::world::level::material::FluidStateExt;
use mc_world::heightmap::HeightmapType;
use std::sync::Arc;

pub struct RootSystemFeature {
    pub tree_feature: Holder<PlacedFeature>,
    pub required_vertical_space_for_tree: i32,
    pub level_test_distance: i32,
    pub max_level_deviation: i32,
    pub root_radius: i32,
    pub root_replaceable: BlockSet,
    pub root_state_provider: Holder<dyn BlockStateProvider>,
    pub root_placement_attempts: i32,
    pub root_column_max_height: i32,
    pub hanging_root_radius: i32,
    pub hanging_roots_vertical_span: i32,
    pub hanging_root_state_provider: Holder<dyn BlockStateProvider>,
    pub hanging_root_placement_attempts: i32,
    pub allowed_vertical_water_for_tree: i32,
    pub allowed_tree_position: Arc<dyn BlockPredicate>,
}

impl RootSystemFeature {
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(RootSystemFeature {
            tree_feature: fields.required("feature", |v| ctx.placed_feature(v))?,
            required_vertical_space_for_tree: fields
                .required("required_vertical_space_for_tree", |v| {
                    parse_int_range(v, 1, 64)
                })?,
            level_test_distance: fields
                .required("level_test_distance", |v| parse_int_range(v, 0, 16))?,
            max_level_deviation: fields
                .required("max_level_deviation", |v| parse_int_range(v, 0, 64))?,
            root_radius: fields.required("root_radius", |v| parse_int_range(v, 1, 64))?,
            root_replaceable: fields.required("root_replaceable", BlockSet::from_json)?,
            root_state_provider: fields
                .required("root_state_provider", |v| ctx.block_state_provider(v))?,
            root_placement_attempts: fields
                .required("root_placement_attempts", |v| parse_int_range(v, 1, 256))?,
            root_column_max_height: fields
                .required("root_column_max_height", |v| parse_int_range(v, 1, 4096))?,
            hanging_root_radius: fields
                .required("hanging_root_radius", |v| parse_int_range(v, 1, 64))?,
            hanging_roots_vertical_span: fields
                .required("hanging_roots_vertical_span", |v| parse_int_range(v, 1, 16))?,
            hanging_root_state_provider: fields.required("hanging_root_state_provider", |v| {
                ctx.block_state_provider(v)
            })?,
            hanging_root_placement_attempts: fields
                .required("hanging_root_placement_attempts", |v| {
                    parse_int_range(v, 1, 256)
                })?,
            allowed_vertical_water_for_tree: fields
                .required("allowed_vertical_water_for_tree", |v| {
                    parse_int_range(v, 1, 64)
                })?,
            allowed_tree_position: fields
                .required("allowed_tree_position", |v| blockpredicates::decode(v, ctx))?,
        }))
    }

    fn space_for_tree(&self, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let mut column_up_pos = pos;
        for i in 1..=self.required_vertical_space_for_tree {
            column_up_pos = column_up_pos.relative(Direction::Up);
            let state = level.get_block_state(column_up_pos);
            if !is_allowed_tree_space(state, i, self.allowed_vertical_water_for_tree) {
                return false;
            }
        }

        if self.level_test_distance > 0 {
            let mut corner_pos = pos;
            for i in 0..4 {
                corner_pos = corner_pos
                    .relative_n(Direction::from_2d_data_value(i), self.level_test_distance);
                let below = level.get_block_state(corner_pos.below_n(self.max_level_deviation));
                let above = level.get_block_state(corner_pos.above_n(self.max_level_deviation));
                if below.is_air() || !above.is_air() {
                    return false;
                }
                corner_pos = pos;
            }
        }
        true
    }

    fn place_dirt_and_tree(
        &self,
        level: &mut dyn WorldGenLevel,
        generator: &GenEnv,
        random: &mut dyn RandomSource,
        working_pos: &mut BlockPos,
        pos: BlockPos,
    ) -> bool {
        for y in 0..self.root_column_max_height {
            *working_pos = working_pos.relative(Direction::Up);
            if level.get_height_at(
                HeightmapType::WorldSurface,
                working_pos.x(),
                working_pos.z(),
            ) < working_pos.y()
            {
                return false;
            }

            if self.allowed_tree_position.test(&*level, *working_pos)
                && self.space_for_tree(&*level, *working_pos)
            {
                let below_pos = working_pos.below();
                if level.get_fluid_state(below_pos).is_tag("lava")
                    || !level.get_block_state(below_pos).is_solid()
                {
                    return false;
                }

                if self
                    .tree_feature
                    .value()
                    .place(level, generator, random, *working_pos)
                {
                    self.place_dirt(pos, pos.y() + y, level, random);
                    return true;
                }
            }
        }
        false
    }

    fn place_dirt(
        &self,
        origin: BlockPos,
        target_height: i32,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
    ) {
        let origin_x = origin.x();
        let origin_z = origin.z();
        for y in origin.y()..target_height {
            self.place_rooted_dirt(
                level,
                random,
                origin_x,
                origin_z,
                BlockPos::new(origin_x, y, origin_z),
            );
        }
    }

    fn place_rooted_dirt(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        origin_x: i32,
        origin_z: i32,
        start_pos: BlockPos,
    ) {
        let mut working_pos = start_pos;
        for _ in 0..self.root_placement_attempts {
            let dx = random.next_int_bounded(self.root_radius)
                - random.next_int_bounded(self.root_radius);
            let dz = random.next_int_bounded(self.root_radius)
                - random.next_int_bounded(self.root_radius);
            working_pos = working_pos.offset(dx, 0, dz);
            if self
                .root_replaceable
                .contains(level.get_block_state(working_pos).block())
            {
                let state =
                    self.root_state_provider
                        .value()
                        .get_state(&*level, random, working_pos);
                level.set_block_flags(working_pos, state, 2);
            }
            working_pos = BlockPos::new(origin_x, working_pos.y(), origin_z);
        }
    }

    fn place_roots(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        pos: BlockPos,
    ) {
        for _ in 0..self.hanging_root_placement_attempts {
            let dx = random.next_int_bounded(self.hanging_root_radius)
                - random.next_int_bounded(self.hanging_root_radius);
            let dy = random.next_int_bounded(self.hanging_roots_vertical_span)
                - random.next_int_bounded(self.hanging_roots_vertical_span);
            let dz = random.next_int_bounded(self.hanging_root_radius)
                - random.next_int_bounded(self.hanging_root_radius);
            let working_pos = pos.offset(dx, dy, dz);
            if level.is_empty_block(working_pos) {
                let target_state: BlockState = self.hanging_root_state_provider.value().get_state(
                    &*level,
                    random,
                    working_pos,
                );
                if target_state.can_survive(&*level, working_pos)
                    && level
                        .get_block_state(working_pos.above())
                        .is_face_sturdy_at(&*level, working_pos, Direction::Down)
                {
                    level.set_block_flags(working_pos, target_state, 2);
                }
            }
        }
    }
}

fn is_allowed_tree_space(
    state: BlockState,
    blocks_above_origin: i32,
    allowed_vertical_water_height: i32,
) -> bool {
    if state.is_air() {
        true
    } else {
        let blocks_above_ground = blocks_above_origin + 1;
        blocks_above_ground <= allowed_vertical_water_height
            && state.get_fluid_state().is_tag("water")
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static ROOT_SYSTEM: TypeEntry<dyn Feature> = TypeEntry {
    name: "root_system",
    decode: RootSystemFeature::decode,
};

impl Feature for RootSystemFeature {
    fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        generator: &GenEnv,
        random: &mut dyn RandomSource,
        origin: BlockPos,
    ) -> bool {
        if !level.get_block_state(origin).is_air() {
            return false;
        }
        let mut working_pos = origin;
        if self.place_dirt_and_tree(level, generator, random, &mut working_pos, origin) {
            self.place_roots(level, random, origin);
        }
        true
    }
}
