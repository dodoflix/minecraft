//! `levelgen.feature.FallenTreeFeature` (type `fallen_tree`).

use mc_game_core::world::level::levelgen::feature::tree::java_hash_set::BlockPosHashSet;
use super::tree_decorator::{Context, TreeDecorator};
use super::tree_feature::{TreeSetter, valid_tree_pos};
use super::trunk_placer::with_axis;
use super::TREE_DECORATOR_TYPES;
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::levelgen::codec::{LoadResult, TypeEntry, decode_dispatch, parse_list};
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature, mark_above_for_post_processing};
use mc_game_core::world::level::levelgen::holder::Holder;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::levelgen::stateproviders::BlockStateProvider;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_game_core::world::level::{BlockGetter, LevelAccessor};
use mc_core::direction::{Direction, Plane};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

const FALLEN_LOG_MAX_FALL_HEIGHT_TO_GROUND: i32 = 5;
const FALLEN_LOG_MAX_GROUND_GAP: i32 = 2;

pub struct FallenTreeFeature {
    pub trunk_provider: Holder<dyn BlockStateProvider>,
    pub log_length: IntProvider,
    pub stump_decorators: Vec<Arc<dyn TreeDecorator>>,
    pub log_decorators: Vec<Arc<dyn TreeDecorator>>,
}

impl FallenTreeFeature {
    fn place_fallen_tree(&self, origin: BlockPos, level: &mut dyn WorldGenLevel, random: &mut dyn RandomSource) {
        self.place_stump(level, random, origin);
        let direction = Plane::Horizontal.get_random_direction(random);
        let log_length = self.log_length.sample(random) - 2;
        let distance = 2 + random.next_int_bounded(2);
        let mut log_start_pos = origin.relative_n(direction, distance);
        log_start_pos = self.set_ground_height_for_fallen_log_start_pos(level, log_start_pos);
        if self.can_place_entire_fallen_log(level, log_length, log_start_pos, direction) {
            self.place_fallen_log(level, random, log_length, log_start_pos, direction);
        }
    }

    fn set_ground_height_for_fallen_log_start_pos(&self, level: &dyn LevelAccessor, mut log_start_pos: BlockPos) -> BlockPos {
        log_start_pos = log_start_pos.relative_n(Direction::Up, 1);
        for _ in 0..=FALLEN_LOG_MAX_FALL_HEIGHT_TO_GROUND {
            if self.may_place_on(level, log_start_pos) {
                return log_start_pos;
            }
            log_start_pos = log_start_pos.relative(Direction::Down);
        }
        log_start_pos
    }

    fn place_stump(&self, level: &mut dyn WorldGenLevel, random: &mut dyn RandomSource, stump_pos: BlockPos) {
        let stump = self.place_log_block(level, random, stump_pos, &|state| state);
        let mut logs = BlockPosHashSet::new();
        logs.insert(stump);
        self.decorate_logs(level, random, &logs, &self.stump_decorators);
    }

    fn can_place_entire_fallen_log(&self, level: &dyn LevelAccessor, log_length: i32, mut log_start_pos: BlockPos, direction: Direction) -> bool {
        let mut gap_in_ground = 0;
        for _ in 0..log_length {
            if !valid_tree_pos(level, log_start_pos) {
                return false;
            }
            if !self.is_over_solid_ground(level, log_start_pos) {
                gap_in_ground += 1;
                if gap_in_ground > FALLEN_LOG_MAX_GROUND_GAP {
                    return false;
                }
            } else {
                gap_in_ground = 0;
            }
            log_start_pos = log_start_pos.relative(direction);
        }
        true
    }

    fn place_fallen_log(&self, level: &mut dyn WorldGenLevel, random: &mut dyn RandomSource, log_length: i32, mut log_start_pos: BlockPos, direction: Direction) {
        let mut fallen_log = BlockPosHashSet::new();
        let axis = direction.get_axis();
        for _ in 0..log_length {
            fallen_log.insert(self.place_log_block(level, random, log_start_pos, &|state| with_axis(state, axis)));
            log_start_pos = log_start_pos.relative(direction);
        }
        self.decorate_logs(level, random, &fallen_log, &self.log_decorators);
    }

    fn may_place_on(&self, level: &dyn LevelAccessor, block_pos: BlockPos) -> bool {
        valid_tree_pos(level, block_pos) && self.is_over_solid_ground(level, block_pos)
    }

    /// Note vanilla passes the position *above* the ground block to `isFaceSturdy`.
    fn is_over_solid_ground(&self, level: &dyn LevelAccessor, block_pos: BlockPos) -> bool {
        level.get_block_state(block_pos.below()).is_face_sturdy_at(level as &dyn BlockGetter, block_pos, Direction::Up)
    }

    fn place_log_block(&self, level: &mut dyn WorldGenLevel, random: &mut dyn RandomSource, block_pos: BlockPos, sideways_state_modifier: &dyn Fn(BlockState) -> BlockState) -> BlockPos {
        let state = self.trunk_provider.value().get_state(level, random, block_pos);
        level.set_block_and_update(block_pos, sideways_state_modifier(state));
        mark_above_for_post_processing(level, block_pos);
        block_pos
    }

    fn decorate_logs(&self, level: &mut dyn WorldGenLevel, random: &mut dyn RandomSource, logs: &BlockPosHashSet, decorators: &[Arc<dyn TreeDecorator>]) {
        if !decorators.is_empty() {
            let mut setter = TreeSetter::untracked();
            let none = BlockPosHashSet::new();
            let mut context = Context::new(level, &mut setter, random, logs, &none, &none);
            for decorator in decorators {
                decorator.place(&mut context);
            }
        }
    }

    pub fn decode(fields: &mc_game_core::world::level::levelgen::codec::Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<FallenTreeFeature> {
        let decorators = |name: &str| fields.required(name, |v| parse_list(v, |d| decode_dispatch(d, ctx, &TREE_DECORATOR_TYPES)));
        Ok(FallenTreeFeature {
            trunk_provider: fields.required("trunk_provider", |v| ctx.block_state_provider(v))?,
            log_length: fields.required("log_length", |v| IntProvider::from_json_range(v, 0, 16))?,
            stump_decorators: decorators("stump_decorators")?,
            log_decorators: decorators("log_decorators")?,
        })
    }
}

impl Feature for FallenTreeFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        self.place_fallen_tree(origin, level, random);
        true
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static FALLEN_TREE: TypeEntry<dyn Feature> = TypeEntry { name: "fallen_tree", decode: |fields, ctx| Ok(Arc::new(FallenTreeFeature::decode(fields, ctx)?)) };
