//! `FossilFeature` (`fossil`): a fossil template plus its overlay buried 15+ blocks below the surface.

use super::{FEATURE_TYPES, Feature};
use crate::world::level::block::BlockStateExt;
use crate::world::level::levelgen::GenEnv;
use crate::world::level::levelgen::WorldGenLevel;
use crate::world::level::levelgen::Holder;
use crate::world::level::levelgen::codec::{Fields, LoadError, LoadResult, TypeEntry, parse_identifier, parse_int_range, parse_list};
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::structure::templatesystem::structure_place_settings::SharedRandom;
use crate::world::level::levelgen::structure::templatesystem::{StructurePlaceSettings, StructureProcessorList};
use mc_core::bounding_box::BoundingBox;
use mc_core::ident::Identifier;
use mc_core::mirror::Mirror;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_core::rotation::Rotation;
use mc_data::block::Block;
use mc_world::heightmap::HeightmapType;
use serde_json::Value;
use std::sync::Arc;

pub struct FossilFeature {
    pub fossil_structures: Vec<Identifier>,
    pub overlay_structures: Vec<Identifier>,
    pub fossil_processors: Holder<StructureProcessorList>,
    pub overlay_processors: Holder<StructureProcessorList>,
    pub max_empty_corners_allowed: i32,
}

/// `ExtraCodecs.nonEmptyList(Identifier.CODEC.listOf())`.
fn non_empty_identifiers(value: &Value) -> LoadResult<Vec<Identifier>> {
    let list = parse_list(value, parse_identifier)?;
    if list.is_empty() { Err(LoadError::new("List must have contents")) } else { Ok(list) }
}

impl FossilFeature {
    pub fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        let feature = FossilFeature {
            fossil_structures: fields.required("fossil_structures", non_empty_identifiers)?,
            overlay_structures: fields.required("overlay_structures", non_empty_identifiers)?,
            fossil_processors: fields.required("fossil_processors", |v| ctx.processor_list(v))?,
            overlay_processors: fields.required("overlay_processors", |v| ctx.processor_list(v))?,
            max_empty_corners_allowed: fields.required("max_empty_corners_allowed", |v| parse_int_range(v, 0, 7))?,
        };
        if feature.fossil_structures.len() != feature.overlay_structures.len() {
            return Err(LoadError::new("Fossil structure lists must be equal lengths"));
        }
        Ok(Arc::new(feature))
    }
}

/// `countEmptyCorners(level, structureBounds)`.
fn count_empty_corners(level: &dyn WorldGenLevel, structure_bounds: &BoundingBox) -> i32 {
    let mut count = 0;
    structure_bounds.for_all_corners(|pos| {
        let state = level.get_block_state(pos);
        if state.is_air() || state.is_block(Block::LAVA) || state.is_block(Block::WATER) {
            count += 1;
        }
    });
    count
}

impl Feature for FossilFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let rotation = Rotation::get_random(random);
        let fossil_index = random.next_int_bounded(self.fossil_structures.len() as i32) as usize;
        let manager = &generator.structure_template_manager;
        let fossil_base = manager.get_or_create(&self.fossil_structures[fossil_index]);
        let fossil_overlay = manager.get_or_create(&self.overlay_structures[fossil_index]);
        let chunk_pos = ChunkPos::containing(&origin);
        let bounding_box = BoundingBox::new(
            chunk_pos.get_min_block_x() - 16,
            level.get_min_y(),
            chunk_pos.get_min_block_z() - 16,
            chunk_pos.get_max_block_x() + 16,
            level.get_max_y(),
            chunk_pos.get_max_block_z() + 16,
        );
        let size = fossil_base.get_size_rotated(rotation);
        let low_corner = origin.offset(-size.x / 2, 0, -size.z / 2);
        let mut lowest_surface_y = origin.y();
        for xscan in 0..size.x {
            for zscan in 0..size.z {
                lowest_surface_y = lowest_surface_y.min(level.get_height_at(HeightmapType::OceanFloorWg, low_corner.x() + xscan, low_corner.z() + zscan));
            }
        }
        let target_y = (lowest_surface_y - 15 - random.next_int_bounded(10)).max(level.get_min_y() + 10);
        let target_pos = fossil_base.get_zero_position_with_transform(low_corner.at_y(target_y), Mirror::None, rotation);

        SharedRandom::scoped(random, |shared| {
            let mut settings = StructurePlaceSettings::new();
            settings.set_rotation(rotation).set_bounding_box(bounding_box).set_random(Some(shared.clone()));
            if count_empty_corners(level, &fossil_base.get_bounding_box(&settings, target_pos)) > self.max_empty_corners_allowed {
                return false;
            }
            settings.clear_processors();
            for processor in self.fossil_processors.value().list() {
                settings.add_processor(processor.clone());
            }
            fossil_base.place_in_world(level, target_pos, target_pos, &settings, &mut shared.clone(), 260);
            settings.clear_processors();
            for processor in self.overlay_processors.value().list() {
                settings.add_processor(processor.clone());
            }
            fossil_overlay.place_in_world(level, target_pos, target_pos, &settings, &mut shared.clone(), 260);
            true
        })
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static FOSSIL: TypeEntry<dyn Feature> = TypeEntry { name: "fossil", decode: FossilFeature::decode };
