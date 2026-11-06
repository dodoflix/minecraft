//! `UnderwaterMagmaFeature`.

use crate::world::level::levelgen::column::Column;
use mc_core::bounding_box::BoundingBox;
use mc_core::direction::{Direction, Plane};
use mc_core::pos::{BlockPos, Vec3i};
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::block::helpers::is_shape_full;
use mc_game_core::world::level::levelgen::codec::{
    Fields, LoadResult, TypeEntry, parse_float_range, parse_int_range,
};
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_game_core::world::level::{BlockGetter, LevelAccessor};
use std::sync::Arc;

pub struct UnderwaterMagmaFeature {
    pub floor_search_range: i32,
    pub placement_radius_around_floor: i32,
    pub placement_probability_per_valid_position: f32,
}

impl UnderwaterMagmaFeature {
    fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(UnderwaterMagmaFeature {
            floor_search_range: fields
                .required("floor_search_range", |v| parse_int_range(v, 0, 512))?,
            placement_radius_around_floor: fields
                .required("placement_radius_around_floor", |v| {
                    parse_int_range(v, 0, 64)
                })?,
            placement_probability_per_valid_position: fields
                .required("placement_probability_per_valid_position", |v| {
                    parse_float_range(v, 0.0, 1.0)
                })?,
        }))
    }

    fn get_floor_y(&self, level: &dyn BlockGetter, origin: BlockPos) -> Option<i32> {
        let inside_column = |state: BlockState| state.block() == Block::WATER;
        let valid_edge = |state: BlockState| state.block() != Block::WATER;
        Column::scan(
            level,
            origin,
            self.floor_search_range,
            inside_column,
            valid_edge,
        )
        .and_then(|column| column.get_floor())
    }

    fn is_valid_placement(&self, level: &dyn LevelAccessor, pos: BlockPos) -> bool {
        if !is_water_or_air(level.get_block_state(pos))
            && !self.is_visible_from_outside(level, pos.below(), Direction::Up)
        {
            for neighbour_dir in Plane::Horizontal {
                if self.is_visible_from_outside(
                    level,
                    pos.relative(neighbour_dir),
                    neighbour_dir.get_opposite(),
                ) {
                    return false;
                }
            }
            true
        } else {
            false
        }
    }

    fn is_visible_from_outside(
        &self,
        level: &dyn LevelAccessor,
        pos: BlockPos,
        covered_direction: Direction,
    ) -> bool {
        let state = level.get_block_state(pos);
        let face_occlusion_shape = state.get_face_occlusion_shape(covered_direction);
        // `shape == Shapes.empty() || !isShapeFullBlock(shape)`; an empty shape is never a full block
        !is_shape_full(&face_occlusion_shape)
    }
}

fn is_water_or_air(state: BlockState) -> bool {
    state.block() == Block::WATER || state.is_air()
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static UNDERWATER_MAGMA: TypeEntry<dyn Feature> = TypeEntry {
    name: "underwater_magma",
    decode: UnderwaterMagmaFeature::decode,
};

impl Feature for UnderwaterMagmaFeature {
    fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        _generator: &GenEnv,
        random: &mut dyn RandomSource,
        origin: BlockPos,
    ) -> bool {
        let Some(floor_y) = self.get_floor_y(&*level, origin) else {
            return false;
        };
        let floor_pos = BlockPos::new(origin.x(), floor_y, origin.z());
        let r = self.placement_radius_around_floor;
        let radius = Vec3i::new(r, r, r);
        let bounds =
            BoundingBox::from_corners(&floor_pos.subtract(&radius), &floor_pos.offset_vec(&radius));
        let mut placed = 0;
        for pos in BlockPos::between_closed_box(&bounds) {
            if random.next_float() < self.placement_probability_per_valid_position
                && self.is_valid_placement(&*level, pos)
            {
                level.set_block_flags(pos, Block::MAGMA_BLOCK.default_state(), 2);
                placed += 1;
            }
        }
        placed > 0
    }
}
