//! `FeaturePoolElement` (`feature_pool_element`): places a placed feature at the jigsaw.

use mc_core::bounding_box::BoundingBox;
use mc_core::direction::{Direction, FrontAndTop};
use mc_core::pos::{BlockPos, Vec3i};
use mc_core::random::RandomSource;
use mc_core::rotation::Rotation;
use mc_data::block::Block;
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::holder::Holder;
use mc_game_core::world::level::levelgen::placement::PlacedFeature;
use mc_game_core::world::level::levelgen::structure::StructureManager;
use mc_game_core::world::level::levelgen::structure::pools::structure_pool_element::{
    ElementCodecCtx, STRUCTURE_POOL_ELEMENT_TYPES, StructurePoolElement, StructurePoolElementType,
    decode_placed_feature, parse_projection,
};
use mc_game_core::world::level::levelgen::structure::pools::structure_template_pool::{
    Projection, empty_pool_id,
};
use mc_game_core::world::level::levelgen::structure::templatesystem::structure_template::{
    JigsawBlockInfo, JointType,
};
use mc_game_core::world::level::levelgen::structure::templatesystem::{
    LiquidSettings, StructureTemplateManager,
};
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_nbt::{CompoundTag, Tag};
use std::fmt;
use std::sync::Arc;

pub struct FeaturePoolElement {
    feature: Holder<PlacedFeature>,
    projection: Projection,
}

impl FeaturePoolElement {
    pub fn new(feature: Holder<PlacedFeature>, projection: Projection) -> FeaturePoolElement {
        FeaturePoolElement {
            feature,
            projection,
        }
    }
}

impl fmt::Display for FeaturePoolElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Feature[{:?}]", self.feature)
    }
}

impl StructurePoolElement for FeaturePoolElement {
    fn get_size(&self, _manager: &StructureTemplateManager, _rotation: Rotation) -> Vec3i {
        Vec3i::new(0, 0, 0)
    }

    fn get_shuffled_jigsaw_blocks(
        &self,
        _manager: &StructureTemplateManager,
        position: BlockPos,
        _rotation: Rotation,
        _random: &mut dyn RandomSource,
    ) -> Vec<JigsawBlockInfo> {
        let jigsaw = Block::by_name("minecraft:jigsaw").expect("jigsaw block");
        let orientation =
            FrontAndTop::from_front_and_top(Direction::Down, Direction::South).expect("down_south");
        let state = jigsaw
            .default_state()
            .with_property_str("orientation", orientation.get_serialized_name())
            .expect("jigsaw orientation property");
        vec![JigsawBlockInfo {
            pos: position,
            state,
            joint_type: JointType::Rollable,
            name: None,
            pool: empty_pool_id(),
            target: empty_pool_id(),
            placement_priority: 0,
            selection_priority: 0,
        }]
    }

    fn get_bounding_box(
        &self,
        manager: &StructureTemplateManager,
        position: BlockPos,
        rotation: Rotation,
    ) -> BoundingBox {
        let size = self.get_size(manager, rotation);
        BoundingBox::new(
            position.x(),
            position.y(),
            position.z(),
            position.x() + size.x,
            position.y() + size.y,
            position.z() + size.z,
        )
    }

    fn place(
        &self,
        _manager: &StructureTemplateManager,
        level: &mut dyn WorldGenLevel,
        _structure_manager: &StructureManager,
        generator: &GenEnv,
        position: BlockPos,
        _reference_pos: BlockPos,
        _rotation: Rotation,
        _chunk_bb: &BoundingBox,
        random: &mut dyn RandomSource,
        _liquid_settings: LiquidSettings,
        _keep_jigsaws: bool,
    ) -> bool {
        // The pool registry decodes with its own holder cells: a reference to a placed feature is bound on first use from the
        // level's feature registry.
        if !self.feature.is_bound()
            && let Some(id) = self.feature.id()
            && let Some(bound) = generator.registries.placed_feature(id)
        {
            self.feature.bind(bound.value().clone());
        }
        self.feature
            .value()
            .place(level, generator, random, position)
    }

    fn get_type(&self) -> &'static str {
        "feature_pool_element"
    }

    fn get_projection(&self) -> Projection {
        self.projection
    }

    fn with_projection(&self, projection: Projection) -> Arc<dyn StructurePoolElement> {
        Arc::new(FeaturePoolElement {
            feature: self.feature.clone(),
            projection,
        })
    }

    fn encode_fields(&self, tag: &mut CompoundTag) {
        // `PlacedFeature.CODEC` encodes a reference as its id (a direct holder is never produced by the data pack).
        tag.put(
            "feature",
            Tag::String(
                self.feature
                    .id()
                    .map_or_else(String::new, ToString::to_string),
            ),
        );
        tag.put_string("projection", self.projection.get_name());
    }
}

#[linkme::distributed_slice(STRUCTURE_POOL_ELEMENT_TYPES)]
static TYPE: StructurePoolElementType = StructurePoolElementType {
    name: "feature_pool_element",
    decode: |f, ctx: &ElementCodecCtx<'_, '_>| {
        Ok(Arc::new(FeaturePoolElement {
            feature: f.required("feature", |v| decode_placed_feature(v, ctx))?,
            projection: f.required("projection", parse_projection)?,
        }))
    },
};
