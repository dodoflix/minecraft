//! `ListPoolElement` (`list_pool_element`): several elements placed at the same position; the first one provides the
//! jigsaw blocks.

use super::structure_pool_element::{ElementCodecCtx, STRUCTURE_POOL_ELEMENT_TYPES, StructurePoolElement, StructurePoolElementType, decode_element, encode_element, parse_projection};
use super::structure_template_pool::Projection;
use crate::world::level::levelgen::codec::*;
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::structure::StructureManager;
use crate::world::level::levelgen::structure::templatesystem::structure_template::JigsawBlockInfo;
use crate::world::level::levelgen::structure::templatesystem::{LiquidSettings, StructureTemplateManager};
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::bounding_box::BoundingBox;
use mc_core::pos::{BlockPos, Vec3i};
use mc_core::random::RandomSource;
use mc_core::rotation::Rotation;
use mc_nbt::{CompoundTag, ListTag, Tag};
use std::fmt;
use std::sync::Arc;

pub struct ListPoolElement {
    elements: Vec<Arc<dyn StructurePoolElement>>,
    projection: Projection,
}

impl ListPoolElement {
    /// `new ListPoolElement(elements, projection)`: panics on an empty list (`IllegalArgumentException`). The projection is
    /// set on every element.
    pub fn new(elements: Vec<Arc<dyn StructurePoolElement>>, projection: Projection) -> ListPoolElement {
        assert!(!elements.is_empty(), "Elements are empty");
        ListPoolElement { elements: elements.iter().map(|e| e.with_projection(projection)).collect(), projection }
    }

    /// `getElements()`.
    pub fn get_elements(&self) -> &[Arc<dyn StructurePoolElement>] {
        &self.elements
    }
}

impl fmt::Display for ListPoolElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "List[{}]", self.elements.iter().map(ToString::to_string).collect::<Vec<_>>().join(", "))
    }
}

impl StructurePoolElement for ListPoolElement {
    fn get_size(&self, manager: &StructureTemplateManager, rotation: Rotation) -> Vec3i {
        let (mut size_x, mut size_y, mut size_z) = (0, 0, 0);
        for element in &self.elements {
            let size = element.get_size(manager, rotation);
            size_x = size_x.max(size.x);
            size_y = size_y.max(size.y);
            size_z = size_z.max(size.z);
        }
        Vec3i::new(size_x, size_y, size_z)
    }

    fn get_shuffled_jigsaw_blocks(&self, manager: &StructureTemplateManager, position: BlockPos, rotation: Rotation, random: &mut dyn RandomSource) -> Vec<JigsawBlockInfo> {
        self.elements[0].get_shuffled_jigsaw_blocks(manager, position, rotation, random)
    }

    fn get_bounding_box(&self, manager: &StructureTemplateManager, position: BlockPos, rotation: Rotation) -> BoundingBox {
        BoundingBox::encapsulating_boxes(self.elements.iter().filter(|e| !e.is_empty_element()).map(|e| e.get_bounding_box(manager, position, rotation)))
            .expect("Unable to calculate boundingbox for ListPoolElement")
    }

    fn place(
        &self,
        manager: &StructureTemplateManager,
        level: &mut dyn WorldGenLevel,
        structure_manager: &StructureManager,
        generator: &GenEnv,
        position: BlockPos,
        reference_pos: BlockPos,
        rotation: Rotation,
        chunk_bb: &BoundingBox,
        random: &mut dyn RandomSource,
        liquid_settings: LiquidSettings,
        keep_jigsaws: bool,
    ) -> bool {
        self.elements.iter().all(|element| {
            element.place(manager, level, structure_manager, generator, position, reference_pos, rotation, chunk_bb, random, liquid_settings, keep_jigsaws)
        })
    }

    fn get_type(&self) -> &'static str {
        "list_pool_element"
    }

    fn get_projection(&self) -> Projection {
        self.projection
    }

    fn with_projection(&self, projection: Projection) -> Arc<dyn StructurePoolElement> {
        Arc::new(ListPoolElement::new(self.elements.clone(), projection))
    }

    fn encode_fields(&self, tag: &mut CompoundTag) {
        let mut list = ListTag::new();
        for element in &self.elements {
            list.push(Tag::Compound(encode_element(&**element)));
        }
        tag.put("elements", Tag::List(list));
        tag.put_string("projection", self.projection.get_name());
    }
}

#[linkme::distributed_slice(STRUCTURE_POOL_ELEMENT_TYPES)]
static TYPE: StructurePoolElementType = StructurePoolElementType {
    name: "list_pool_element",
    decode: |f, ctx: &ElementCodecCtx<'_, '_>| {
        let elements = f.required("elements", |v| parse_list(v, |e| decode_element(e, ctx)))?;
        let projection = f.required("projection", parse_projection)?;
        // `new ListPoolElement(..)` throws `IllegalArgumentException("Elements are empty")` out of the codec
        if elements.is_empty() {
            return Err(LoadError::new("Elements are empty"));
        }
        Ok(Arc::new(ListPoolElement::new(elements, projection)))
    },
};
