//! `WaterloggedVegetationPatchFeature`.

use super::vegetation_patch_feature::{self, VegetationPatch, VegetationPatchFeature};
use super::{FEATURE_TYPES, Feature};
use crate::world::level::block::BlockStateExt;
use crate::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::java_util::JavaHashSet;
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use crate::world::level::{BlockGetter};
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::props;
use std::sync::Arc;

pub struct WaterloggedVegetationPatchFeature {
    base: VegetationPatchFeature,
}

impl WaterloggedVegetationPatchFeature {
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(WaterloggedVegetationPatchFeature { base: VegetationPatchFeature::decode_config(fields, ctx)? }))
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static WATERLOGGED_VEGETATION_PATCH: TypeEntry<dyn Feature> =
    TypeEntry { name: "waterlogged_vegetation_patch", decode: WaterloggedVegetationPatchFeature::decode };

fn is_exposed_direction(level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> bool {
    let test_pos = pos.relative(direction);
    !level.get_block_state(test_pos).is_face_sturdy_at(level, test_pos, direction.get_opposite())
}

fn is_exposed(level: &dyn BlockGetter, pos: BlockPos) -> bool {
    is_exposed_direction(level, pos, Direction::North)
        || is_exposed_direction(level, pos, Direction::East)
        || is_exposed_direction(level, pos, Direction::South)
        || is_exposed_direction(level, pos, Direction::West)
        || is_exposed_direction(level, pos, Direction::Down)
}

impl VegetationPatch for WaterloggedVegetationPatchFeature {
    fn config(&self) -> &VegetationPatchFeature {
        &self.base
    }

    fn place_ground_patch(&self, level: &mut dyn WorldGenLevel, random: &mut dyn RandomSource, origin: BlockPos, x_radius: i32, z_radius: i32) -> JavaHashSet {
        let surface = self.base.place_ground_patch(level, random, origin, x_radius, z_radius);
        let mut water_surface = JavaHashSet::new();
        for surface_pos in surface.iter() {
            if !is_exposed(&*level, *surface_pos) {
                water_surface.add(*surface_pos);
            }
        }
        for surface_pos in water_surface.iter() {
            level.set_block_flags(*surface_pos, Block::WATER.default_state(), 2);
        }
        water_surface
    }

    fn place_vegetation(&self, level: &mut dyn WorldGenLevel, generator: &GenEnv, random: &mut dyn RandomSource, placement_pos: BlockPos) -> bool {
        if VegetationPatch::place_vegetation(&self.base, level, generator, random, placement_pos.below()) {
            let placed = level.get_block_state(placement_pos);
            if placed.has_property(props::WATERLOGGED) && !placed.get(props::WATERLOGGED) {
                level.set_block_flags(placement_pos, placed.set(props::WATERLOGGED, true), 2);
            }
            true
        } else {
            false
        }
    }
}

impl Feature for WaterloggedVegetationPatchFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        vegetation_patch_feature::place(self, level, generator, random, origin)
    }
}
