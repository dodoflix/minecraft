//! `SinglePoolElement` (`single_pool_element`) and the shared base of `LegacySinglePoolElement`
//! (`legacy_single_pool_element`): one structure template with its processor list.
//!
//! Vanilla's `Either<Identifier, StructureTemplate>` template: the runtime-template alternative only exists for
//! game tests / data generation and cannot be serialized, so only the identifier is held.

use super::structure_pool_element::{
    ElementCodecCtx, ProcessorListRef, STRUCTURE_POOL_ELEMENT_TYPES, StructurePoolElement, StructurePoolElementType, parse_projection,
};
use super::structure_template_pool::Projection;
use crate::world::level::levelgen::codec::*;
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::feature::tree::shuffle;
use crate::world::level::levelgen::structure::StructureManager;
use crate::world::level::levelgen::structure::templatesystem::block_ignore_processor::{STRUCTURE_AND_AIR, STRUCTURE_BLOCK};
use crate::world::level::levelgen::structure::templatesystem::jigsaw_replacement_processor;
use crate::world::level::levelgen::structure::templatesystem::structure_template::{JigsawBlockInfo, StructureBlockInfo, process_block_infos};
use crate::world::level::levelgen::structure::templatesystem::{LiquidSettings, StructurePlaceSettings, StructureTemplate, StructureTemplateManager};
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::bounding_box::BoundingBox;
use mc_core::ident::Identifier;
use mc_core::pos::{BlockPos, Vec3i};
use mc_core::random::RandomSource;
use mc_core::rotation::Rotation;
use mc_data::block::Block;
use mc_nbt::CompoundTag;
use std::fmt;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Variant {
    Single,
    Legacy,
}

pub struct SinglePoolElement {
    variant: Variant,
    template: Identifier,
    processors: ProcessorListRef,
    projection: Projection,
    override_liquid_settings: Option<LiquidSettings>,
}

impl SinglePoolElement {
    /// `new SinglePoolElement(template, processors, projection, overrideLiquidSettings)`.
    pub fn new(template: Identifier, processors: ProcessorListRef, projection: Projection, override_liquid_settings: Option<LiquidSettings>) -> SinglePoolElement {
        SinglePoolElement { variant: Variant::Single, template, processors, projection, override_liquid_settings }
    }

    /// `new LegacySinglePoolElement(template, processors, projection, overrideLiquidSettings)`.
    pub fn new_legacy(template: Identifier, processors: ProcessorListRef, projection: Projection, override_liquid_settings: Option<LiquidSettings>) -> SinglePoolElement {
        SinglePoolElement { variant: Variant::Legacy, template, processors, projection, override_liquid_settings }
    }

    /// `getTemplateLocation()`.
    pub fn get_template_location(&self) -> &Identifier {
        &self.template
    }

    fn get_template(&self, manager: &StructureTemplateManager) -> Arc<StructureTemplate> {
        manager.get_or_create(&self.template)
    }

    /// `getDataMarkers(structureTemplateManager, position, rotation, absolute)`.
    pub fn get_data_markers(&self, manager: &StructureTemplateManager, position: BlockPos, rotation: Rotation, absolute: bool) -> Vec<StructureBlockInfo> {
        let template = self.get_template(manager);
        let mut settings = StructurePlaceSettings::new();
        settings.set_rotation(rotation);
        let structure_block = Block::by_name("minecraft:structure_block").expect("structure_block");
        template
            .filter_blocks_abs(position, &settings, structure_block, absolute)
            .into_iter()
            // `mode` is the legacy enum name (`StructureMode.LEGACY_CODEC`)
            .filter(|info| info.nbt.as_ref().is_some_and(|nbt| nbt.get_string("mode") == Some("DATA")))
            .collect()
    }

    /// `getSettings(rotation, chunkBB, liquidSettings, keepJigsaws)` (the legacy element swaps the block ignore processor).
    fn get_settings(&self, rotation: Rotation, chunk_bb: &BoundingBox, liquid_settings: LiquidSettings, keep_jigsaws: bool) -> StructurePlaceSettings {
        let mut settings = StructurePlaceSettings::new();
        settings.set_bounding_box(*chunk_bb);
        settings.set_rotation(rotation);
        settings.set_known_shape(true);
        settings.set_ignore_entities(false);
        settings.add_processor(STRUCTURE_BLOCK.clone());
        settings.set_finalize_entities(true);
        settings.set_liquid_settings(self.override_liquid_settings.unwrap_or(liquid_settings));
        if !keep_jigsaws {
            settings.add_processor(jigsaw_replacement_processor::INSTANCE.clone());
        }
        for processor in self.processors.holder.value().list() {
            settings.add_processor(processor.clone());
        }
        for processor in self.projection.get_processors() {
            settings.add_processor(processor.clone());
        }
        if self.variant == Variant::Legacy {
            settings.pop_processor(&STRUCTURE_BLOCK);
            settings.add_processor(STRUCTURE_AND_AIR.clone());
        }
        settings
    }
}

impl fmt::Display for SinglePoolElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.variant {
            Variant::Single => write!(f, "Single[Left[{}]]", self.template),
            Variant::Legacy => write!(f, "LegacySingle[Left[{}]]", self.template),
        }
    }
}

impl StructurePoolElement for SinglePoolElement {
    fn get_size(&self, manager: &StructureTemplateManager, rotation: Rotation) -> Vec3i {
        self.get_template(manager).get_size_rotated(rotation)
    }

    fn get_shuffled_jigsaw_blocks(&self, manager: &StructureTemplateManager, position: BlockPos, rotation: Rotation, random: &mut dyn RandomSource) -> Vec<JigsawBlockInfo> {
        let mut jigsaws = self.get_template(manager).get_jigsaws(position, rotation);
        shuffle(&mut jigsaws, random);
        sort_by_selection_priority(&mut jigsaws);
        jigsaws
    }

    fn get_bounding_box(&self, manager: &StructureTemplateManager, position: BlockPos, rotation: Rotation) -> BoundingBox {
        let mut settings = StructurePlaceSettings::new();
        settings.set_rotation(rotation);
        self.get_template(manager).get_bounding_box(&settings, position)
    }

    fn place(
        &self,
        manager: &StructureTemplateManager,
        level: &mut dyn WorldGenLevel,
        _structure_manager: &StructureManager,
        _generator: &GenEnv,
        position: BlockPos,
        reference_pos: BlockPos,
        rotation: Rotation,
        chunk_bb: &BoundingBox,
        random: &mut dyn RandomSource,
        liquid_settings: LiquidSettings,
        keep_jigsaws: bool,
    ) -> bool {
        let template = self.get_template(manager);
        let settings = self.get_settings(rotation, chunk_bb, liquid_settings, keep_jigsaws);
        if !template.place_in_world(level, position, reference_pos, &settings, random, 18) {
            return false;
        }
        let markers = self.get_data_markers(manager, position, rotation, false);
        for data_marker in process_block_infos(&*level, position, reference_pos, &settings, &markers) {
            self.handle_data_marker(level, &data_marker, position, rotation, random, chunk_bb);
        }
        true
    }

    fn get_type(&self) -> &'static str {
        match self.variant {
            Variant::Single => "single_pool_element",
            Variant::Legacy => "legacy_single_pool_element",
        }
    }

    fn get_projection(&self) -> Projection {
        self.projection
    }

    fn with_projection(&self, projection: Projection) -> Arc<dyn StructurePoolElement> {
        Arc::new(SinglePoolElement {
            variant: self.variant,
            template: self.template.clone(),
            processors: self.processors.clone(),
            projection,
            override_liquid_settings: self.override_liquid_settings,
        })
    }

    fn encode_fields(&self, tag: &mut CompoundTag) {
        tag.put_string("location", self.template.to_string());
        tag.put("processors", self.processors.encode());
        tag.put_string("projection", self.projection.get_name());
        if let Some(liquid) = self.override_liquid_settings {
            tag.put_string("override_liquid_settings", liquid.get_serialized_name());
        }
    }
}

/// `SinglePoolElement.sortBySelectionPriority(blocks)`: highest selection priority first (stable).
pub fn sort_by_selection_priority(blocks: &mut [JigsawBlockInfo]) {
    blocks.sort_by(|a, b| b.selection_priority.cmp(&a.selection_priority));
}

fn decode_single(fields: &Fields<'_>, ctx: &ElementCodecCtx<'_, '_>, legacy: bool) -> LoadResult<Arc<dyn StructurePoolElement>> {
    let template = fields.required("location", parse_identifier)?;
    let processors = fields.required("processors", |v| ProcessorListRef::decode(v, ctx))?;
    let projection = fields.required("projection", parse_projection)?;
    let override_liquid = fields.optional("override_liquid_settings", |v| {
        let name = parse_str(v)?;
        LiquidSettings::by_name(name).ok_or_else(|| LoadError::new(format!("Unknown element name:{name}")))
    })?;
    Ok(Arc::new(if legacy {
        SinglePoolElement::new_legacy(template, processors, projection, override_liquid)
    } else {
        SinglePoolElement::new(template, processors, projection, override_liquid)
    }))
}

#[linkme::distributed_slice(STRUCTURE_POOL_ELEMENT_TYPES)]
static SINGLE: StructurePoolElementType = StructurePoolElementType { name: "single_pool_element", decode: |f, ctx| decode_single(f, ctx, false) };

#[linkme::distributed_slice(STRUCTURE_POOL_ELEMENT_TYPES)]
static LEGACY: StructurePoolElementType = StructurePoolElementType { name: "legacy_single_pool_element", decode: |f, ctx| decode_single(f, ctx, true) };
