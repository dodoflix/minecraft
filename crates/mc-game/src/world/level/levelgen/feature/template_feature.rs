//! `TemplateFeature` (`template`): places a structure template from the data pack (desert well, sulfur spring).

use super::{FEATURE_TYPES, Feature};
use crate::world::level::levelgen::GenEnv;
use crate::world::level::levelgen::WorldGenLevel;
use crate::world::level::levelgen::Holder;
use crate::world::level::levelgen::codec::{Fields, LoadError, LoadResult, TypeEntry, parse_identifier, parse_list, parse_str};
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::structure::templatesystem::structure_place_settings::SharedRandom;
use crate::world::level::levelgen::structure::templatesystem::{StructurePlaceSettings, StructureProcessorList, StructureTemplate};
use mc_core::direction::Axis;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_core::pos::Vec3i;
use mc_core::random::RandomSource;
use mc_core::rotation::Rotation;
use mc_worldgen::provider::WeightedList;
use serde_json::Value;
use std::sync::Arc;

/// `TemplateFeature.TemplateEntry`.
pub struct TemplateEntry {
    pub template: Identifier,
    pub rotations: Vec<Rotation>,
}

impl TemplateEntry {
    pub fn from_json(value: &Value) -> LoadResult<TemplateEntry> {
        let fields = Fields::new(value)?;
        Ok(TemplateEntry {
            template: fields.required("id", parse_identifier)?,
            rotations: fields
                .optional("rotations", |v| {
                    parse_list(v, |r| {
                        let name = parse_str(r)?;
                        Rotation::by_name(name).ok_or_else(|| LoadError::new(format!("Unknown element name:{name}")))
                    })
                })?
                .unwrap_or_else(|| Rotation::ALL.to_vec()),
        })
    }
}

pub struct TemplateFeature {
    pub templates: WeightedList<TemplateEntry>,
    pub processors: Option<Holder<StructureProcessorList>>,
}

impl TemplateFeature {
    pub fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(TemplateFeature {
            templates: fields.required("templates", |v| WeightedList::from_json(v, TemplateEntry::from_json))?,
            processors: fields.optional("processors", |v| ctx.processor_list(v))?,
        }))
    }

    /// `getRotatedOffset(rotation, axis, template)`.
    fn get_rotated_offset(rotation: Rotation, axis: Axis, template: &StructureTemplate) -> Vec3i {
        rotation.rotate(axis.get_negative()).get_unit_vec3i().multiply(template.get_size().get(axis) / 2)
    }
}

impl Feature for TemplateFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let template_entry = self.templates.get_random_or_throw(random);
        let rotation = template_entry.rotations[random.next_int_bounded(template_entry.rotations.len() as i32) as usize];
        let template = generator.structure_template_manager.get_or_create(&template_entry.template);
        let offset_x = Self::get_rotated_offset(rotation, Axis::X, &template);
        let offset_z = Self::get_rotated_offset(rotation, Axis::Z, &template);
        let pos = origin.offset_vec(&offset_x).offset_vec(&offset_z);
        SharedRandom::scoped(random, |shared| {
            let mut settings = StructurePlaceSettings::new();
            settings.set_rotation(rotation).set_random(Some(shared.clone()));
            if let Some(processors) = &self.processors {
                for processor in processors.value().list() {
                    settings.add_processor(processor.clone());
                }
            }
            template.place_in_world(level, pos, pos, &settings, &mut shared.clone(), 3)
        })
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static TEMPLATE: TypeEntry<dyn Feature> = TypeEntry { name: "template", decode: TemplateFeature::decode };
