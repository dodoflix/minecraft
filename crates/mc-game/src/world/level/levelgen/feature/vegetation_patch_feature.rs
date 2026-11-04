//! `VegetationPatchFeature` (the `waterlogged_vegetation_patch` subclass lives in
//! [`super::waterlogged_vegetation_patch_feature`] and overrides [`VegetationPatch::place_ground_patch`] /
//! [`VegetationPatch::place_vegetation`]).

use super::{FEATURE_TYPES, Feature};
use crate::world::level::block::BlockStateExt;
use crate::world::level::levelgen::codec::{BlockSet, Fields, LoadResult, TypeEntry, parse_float_range, parse_int_range};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::java_util::JavaHashSet;
use crate::world::level::levelgen::placement::PlacedFeature;
use crate::world::level::levelgen::placement::cave_surface::CaveSurface;
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::stateproviders::BlockStateProvider;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

pub struct VegetationPatchFeature {
    pub replaceable: BlockSet,
    pub ground_state: Holder<dyn BlockStateProvider>,
    pub vegetation_feature: Holder<PlacedFeature>,
    pub surface: CaveSurface,
    pub depth: IntProvider,
    pub extra_bottom_block_chance: f32,
    pub vertical_range: i32,
    pub vegetation_chance: f32,
    pub xz_radius: IntProvider,
    pub extra_edge_column_chance: f32,
}

impl VegetationPatchFeature {
    /// `VegetationPatchFeature.makeCodec` (shared by both patch types).
    pub fn decode_config(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<VegetationPatchFeature> {
        Ok(VegetationPatchFeature {
            replaceable: fields.required("replaceable", BlockSet::from_json)?,
            ground_state: fields.required("ground_state", |v| ctx.block_state_provider(v))?,
            vegetation_feature: fields.required("vegetation_feature", |v| ctx.placed_feature(v))?,
            surface: fields.required("surface", CaveSurface::from_json)?,
            depth: fields.required("depth", |v| IntProvider::from_json_range(v, 1, 128))?,
            extra_bottom_block_chance: fields.required("extra_bottom_block_chance", |v| parse_float_range(v, 0.0, 1.0))?,
            vertical_range: fields.required("vertical_range", |v| parse_int_range(v, 1, 256))?,
            vegetation_chance: fields.required("vegetation_chance", |v| parse_float_range(v, 0.0, 1.0))?,
            xz_radius: fields.required("xz_radius", IntProvider::from_json)?,
            extra_edge_column_chance: fields.required("extra_edge_column_chance", |v| parse_float_range(v, 0.0, 1.0))?,
        })
    }

    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(Self::decode_config(fields, ctx)?))
    }

    fn base_place_ground_patch(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        origin: BlockPos,
        x_radius: i32,
        z_radius: i32,
    ) -> JavaHashSet {
        let inwards = self.surface.get_direction();
        let outwards = inwards.get_opposite();
        let mut surface = JavaHashSet::new();

        for dx in -x_radius..=x_radius {
            let is_x_edge = dx == -x_radius || dx == x_radius;
            for dz in -z_radius..=z_radius {
                let is_z_edge = dz == -z_radius || dz == z_radius;
                let is_edge = is_x_edge || is_z_edge;
                let is_corner = is_x_edge && is_z_edge;
                let is_edge_but_not_corner = is_edge && !is_corner;
                if !is_corner
                    && (!is_edge_but_not_corner || self.extra_edge_column_chance != 0.0 && !(random.next_float() > self.extra_edge_column_chance))
                {
                    let mut pos = origin.offset(dx, 0, dz);
                    let mut offset = 0;
                    while level.get_block_state(pos).is_air() && offset < self.vertical_range {
                        pos = pos.relative(inwards);
                        offset += 1;
                    }
                    let mut offset = 0;
                    while !level.get_block_state(pos).is_air() && offset < self.vertical_range {
                        pos = pos.relative(outwards);
                        offset += 1;
                    }

                    let mut below_pos = pos.relative(self.surface.get_direction());
                    let below_state = level.get_block_state(below_pos);
                    if level.is_empty_block(pos) && below_state.is_face_sturdy_at(&*level, below_pos, self.surface.get_direction().get_opposite()) {
                        let depth = self.depth.sample(random)
                            + if self.extra_bottom_block_chance > 0.0 && random.next_float() < self.extra_bottom_block_chance { 1 } else { 0 };
                        let ground_pos = below_pos;
                        let ground_placed = self.place_ground(level, random, &mut below_pos, depth);
                        if ground_placed {
                            surface.add(ground_pos);
                        }
                    }
                }
            }
        }
        surface
    }

    fn place_ground(&self, level: &mut dyn WorldGenLevel, random: &mut dyn RandomSource, below_pos: &mut BlockPos, depth: i32) -> bool {
        for i in 0..depth {
            let state_to_place: BlockState = self.ground_state.value().get_state(&*level, random, *below_pos);
            let below_state = level.get_block_state(*below_pos);
            if state_to_place.block() != below_state.block() {
                if !self.replaceable.contains(below_state.block()) {
                    return i != 0;
                }
                level.set_block_flags(*below_pos, state_to_place, 2);
                *below_pos = below_pos.relative(self.surface.get_direction());
            }
        }
        true
    }

    fn base_place_vegetation(&self, level: &mut dyn WorldGenLevel, generator: &GenEnv, random: &mut dyn RandomSource, vegetation_pos: BlockPos) -> bool {
        self.vegetation_feature.value().place(level, generator, random, vegetation_pos.relative(self.surface.get_direction().get_opposite()))
    }
}

/// The overridable parts of `VegetationPatchFeature`.
pub trait VegetationPatch {
    fn config(&self) -> &VegetationPatchFeature;

    /// `placeGroundPatch(level, random, origin, replaceable, xRadius, zRadius)`.
    fn place_ground_patch(&self, level: &mut dyn WorldGenLevel, random: &mut dyn RandomSource, origin: BlockPos, x_radius: i32, z_radius: i32) -> JavaHashSet {
        self.config().base_place_ground_patch(level, random, origin, x_radius, z_radius)
    }

    /// `placeVegetation(level, generator, random, pos)`.
    fn place_vegetation(&self, level: &mut dyn WorldGenLevel, generator: &GenEnv, random: &mut dyn RandomSource, vegetation_pos: BlockPos) -> bool {
        self.config().base_place_vegetation(level, generator, random, vegetation_pos)
    }
}

/// `VegetationPatchFeature.place`.
pub fn place(this: &dyn VegetationPatch, level: &mut dyn WorldGenLevel, generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
    let config = this.config();
    let x_radius = config.xz_radius.sample(random) + 1;
    let z_radius = config.xz_radius.sample(random) + 1;
    let surface = this.place_ground_patch(level, random, origin, x_radius, z_radius);
    for surface_pos in surface.iter() {
        if config.vegetation_chance > 0.0 && random.next_float() < config.vegetation_chance {
            this.place_vegetation(level, generator, random, *surface_pos);
        }
    }
    !surface.is_empty()
}

impl VegetationPatch for VegetationPatchFeature {
    fn config(&self) -> &VegetationPatchFeature {
        self
    }
}

impl Feature for VegetationPatchFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        place(self, level, generator, random, origin)
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static VEGETATION_PATCH: TypeEntry<dyn Feature> = TypeEntry { name: "vegetation_patch", decode: VegetationPatchFeature::decode };
