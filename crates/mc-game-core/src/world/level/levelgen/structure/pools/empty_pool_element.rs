//! `EmptyPoolElement` (`empty_pool_element`): the placeholder that ends a pool's candidate list.

use super::structure_pool_element::{
    STRUCTURE_POOL_ELEMENT_TYPES, StructurePoolElement, StructurePoolElementType,
};
use super::structure_template_pool::Projection;
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::structure::StructureManager;
use crate::world::level::levelgen::structure::templatesystem::structure_template::JigsawBlockInfo;
use crate::world::level::levelgen::structure::templatesystem::{
    LiquidSettings, StructureTemplateManager,
};
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::bounding_box::BoundingBox;
use mc_core::pos::{BlockPos, Vec3i};
use mc_core::random::RandomSource;
use mc_core::rotation::Rotation;
use mc_nbt::CompoundTag;
use std::fmt;
use std::sync::{Arc, LazyLock};

pub struct EmptyPoolElement;

static INSTANCE: LazyLock<Arc<dyn StructurePoolElement>> =
    LazyLock::new(|| Arc::new(EmptyPoolElement));

/// `EmptyPoolElement.INSTANCE`.
pub fn empty_pool_element() -> Arc<dyn StructurePoolElement> {
    INSTANCE.clone()
}

impl fmt::Display for EmptyPoolElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Empty")
    }
}

impl StructurePoolElement for EmptyPoolElement {
    fn get_size(&self, _manager: &StructureTemplateManager, _rotation: Rotation) -> Vec3i {
        Vec3i::new(0, 0, 0)
    }

    fn get_shuffled_jigsaw_blocks(
        &self,
        _manager: &StructureTemplateManager,
        _position: BlockPos,
        _rotation: Rotation,
        _random: &mut dyn RandomSource,
    ) -> Vec<JigsawBlockInfo> {
        Vec::new()
    }

    fn get_bounding_box(
        &self,
        _manager: &StructureTemplateManager,
        _position: BlockPos,
        _rotation: Rotation,
    ) -> BoundingBox {
        panic!("Invalid call to EmptyPoolElement.getBoundingBox, filter me!")
    }

    fn place(
        &self,
        _manager: &StructureTemplateManager,
        _level: &mut dyn WorldGenLevel,
        _structure_manager: &StructureManager,
        _generator: &GenEnv,
        _position: BlockPos,
        _reference_pos: BlockPos,
        _rotation: Rotation,
        _chunk_bb: &BoundingBox,
        _random: &mut dyn RandomSource,
        _liquid_settings: LiquidSettings,
        _keep_jigsaws: bool,
    ) -> bool {
        true
    }

    fn get_type(&self) -> &'static str {
        "empty_pool_element"
    }

    fn get_projection(&self) -> Projection {
        Projection::TerrainMatching
    }

    fn with_projection(&self, _projection: Projection) -> Arc<dyn StructurePoolElement> {
        empty_pool_element()
    }

    fn is_empty_element(&self) -> bool {
        true
    }

    fn encode_fields(&self, _tag: &mut CompoundTag) {}
}

#[linkme::distributed_slice(STRUCTURE_POOL_ELEMENT_TYPES)]
static TYPE: StructurePoolElementType = StructurePoolElementType {
    name: "empty_pool_element",
    decode: |_, _| Ok(empty_pool_element()),
};
