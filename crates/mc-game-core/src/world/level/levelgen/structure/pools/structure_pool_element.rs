//! `StructurePoolElement`: one placeable element of a template pool (a template, a feature, a list of elements or the empty
//! element), its type table (`BuiltInRegistries.STRUCTURE_POOL_ELEMENT`) and the codec context shared by the JSON form
//! (`worldgen/template_pool`) and the NBT form (the `pool_element` of a saved jigsaw piece).

use super::structure_template_pool::Projection;
use crate::world::level::levelgen::codec::*;
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::placement::PlacedFeature;
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::structure::StructureManager;
use crate::world::level::levelgen::structure::templatesystem::structure_template::{
    JigsawBlockInfo, StructureBlockInfo,
};
use crate::world::level::levelgen::structure::templatesystem::{
    LiquidSettings, StructureProcessorList, StructureTemplateManager,
};
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::bounding_box::BoundingBox;
use mc_core::pos::{BlockPos, Vec3i};
use mc_core::random::RandomSource;
use mc_core::rotation::Rotation;
use mc_nbt::{CompoundTag, ListTag, Tag};
use serde_json::{Map, Number, Value};
use std::fmt;
use std::sync::Arc;

/// `StructurePoolElement`.
pub trait StructurePoolElement: fmt::Display + Send + Sync {
    /// `getSize(structureTemplateManager, rotation)`.
    fn get_size(&self, manager: &StructureTemplateManager, rotation: Rotation) -> Vec3i;

    /// `getShuffledJigsawBlocks(structureTemplateManager, position, rotation, random)`.
    fn get_shuffled_jigsaw_blocks(
        &self,
        manager: &StructureTemplateManager,
        position: BlockPos,
        rotation: Rotation,
        random: &mut dyn RandomSource,
    ) -> Vec<JigsawBlockInfo>;

    /// `getBoundingBox(structureTemplateManager, position, rotation)`.
    fn get_bounding_box(
        &self,
        manager: &StructureTemplateManager,
        position: BlockPos,
        rotation: Rotation,
    ) -> BoundingBox;

    /// `place(structureTemplateManager, level, structureManager, generator, position, referencePos, rotation, chunkBB, random,
    /// liquidSettings, keepJigsaws)`.
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
    ) -> bool;

    /// `getType()`: the `element_type` id (path in `minecraft:`).
    fn get_type(&self) -> &'static str;

    /// `handleDataMarker(level, dataMarker, position, rotation, random, chunkBB)`.
    fn handle_data_marker(
        &self,
        _level: &mut dyn WorldGenLevel,
        _data_marker: &StructureBlockInfo,
        _position: BlockPos,
        _rotation: Rotation,
        _random: &mut dyn RandomSource,
        _chunk_bb: &BoundingBox,
    ) {
    }

    /// `getProjection()`.
    fn get_projection(&self) -> Projection;

    /// `setProjection(projection)`: vanilla mutates the element in place (only ever on elements owned by the one list
    /// that sets it); here a copy with the new projection.
    fn with_projection(&self, projection: Projection) -> Arc<dyn StructurePoolElement>;

    /// `getGroundLevelDelta()`.
    fn get_ground_level_delta(&self) -> i32 {
        1
    }

    /// `element == EmptyPoolElement.INSTANCE`.
    fn is_empty_element(&self) -> bool {
        false
    }

    /// The fields of the type's `MapCodec` as NBT (`StructurePoolElement.CODEC.encode` without `element_type`).
    fn encode_fields(&self, tag: &mut CompoundTag);
}

/// `StructurePoolElement.CODEC.encode(element)` over `NbtOps`: `{element_type, ...fields}`.
pub fn encode_element(element: &dyn StructurePoolElement) -> CompoundTag {
    let mut tag = CompoundTag::new();
    tag.put_string("element_type", format!("minecraft:{}", element.get_type()));
    element.encode_fields(&mut tag);
    tag
}

/// What the element codecs resolve holders through: the data-pack decode context while the `worldgen/template_pool`
/// registry loads, the level environment while a saved piece is read back.
pub enum ElementCodecCtx<'a, 'b> {
    Json(&'a DecodeCtx<'b>),
    Nbt(&'a GenEnv),
}

/// A `Holder<StructureProcessorList>` plus the inline definition it was decoded from (vanilla re-encodes a direct holder
/// from its value; the processors have no encoder here, so the source JSON is kept).
#[derive(Clone)]
pub struct ProcessorListRef {
    pub holder: Holder<StructureProcessorList>,
    pub inline: Option<Value>,
}

impl ProcessorListRef {
    /// `StructureProcessorType.LIST_CODEC` of the element codecs.
    pub fn decode(value: &Value, ctx: &ElementCodecCtx<'_, '_>) -> LoadResult<ProcessorListRef> {
        let inline = (!value.is_string()).then(|| value.clone());
        let holder = match ctx {
            ElementCodecCtx::Json(ctx) => ctx.processor_list(value)?,
            ElementCodecCtx::Nbt(env) => match value {
                Value::String(s) => {
                    let id = mc_core::ident::Identifier::read(s).map_err(LoadError::new)?;
                    env.structures
                        .processor_lists
                        .get(&id)
                        .ok_or_else(|| LoadError::new(format!("Failed to get element ResourceKey[minecraft:worldgen/processor_list / {id}]")))?
                        .clone()
                }
                // A saved inline list: only the empty list can be rebuilt without the data-pack decode context.
                other => match other.get("processors") {
                    Some(Value::Array(list)) if list.is_empty() => {
                        Holder::direct(Arc::new(StructureProcessorList::new(Vec::new())))
                    }
                    _ => {
                        return Err(LoadError::new(
                            "inline processor lists with processors cannot be read back from piece NBT",
                        ));
                    }
                },
            },
        };
        Ok(ProcessorListRef { holder, inline })
    }

    /// The NBT form of the holder (`RegistryCodecs.holder` encode): the id of a reference, the list for a direct value.
    pub fn encode(&self) -> Tag {
        match self.holder.id() {
            Some(id) => Tag::String(id.to_string()),
            // `StructureProcessorList.DIRECT_CODEC` always encodes the `{processors: [...]}` form, whichever form was read
            None => match self.inline.as_ref() {
                Some(Value::Array(list)) => json_to_tag(&Value::Object(Map::from_iter([(
                    "processors".to_owned(),
                    Value::Array(list.clone()),
                )]))),
                Some(object) => json_to_tag(object),
                None => json_to_tag(&Value::Object(Map::from_iter([(
                    "processors".to_owned(),
                    Value::Array(Vec::new()),
                )]))),
            },
        }
    }
}

/// `PlacedFeature.CODEC` of the feature element: a reference resolves against the feature registry (bound lazily, see
/// `FeaturePoolElement`).
pub fn decode_placed_feature(
    value: &Value,
    ctx: &ElementCodecCtx<'_, '_>,
) -> LoadResult<Holder<PlacedFeature>> {
    match ctx {
        ElementCodecCtx::Json(ctx) => ctx.placed_feature(value),
        ElementCodecCtx::Nbt(env) => match value {
            Value::String(s) => {
                let id = mc_core::ident::Identifier::read(s).map_err(LoadError::new)?;
                env.registries
                    .placed_feature(&id)
                    .cloned()
                    .ok_or_else(|| LoadError::new(format!("Failed to get element ResourceKey[minecraft:worldgen/placed_feature / {id}]")))
            }
            _ => Err(LoadError::new(
                "inline placed features cannot be read back from piece NBT",
            )),
        },
    }
}

/// `StructurePoolElementType`: the registered type and the decoder of its `MapCodec` (it receives the whole object).
pub struct StructurePoolElementType {
    pub name: &'static str,
    pub decode:
        fn(&Fields<'_>, &ElementCodecCtx<'_, '_>) -> LoadResult<Arc<dyn StructurePoolElement>>,
}

#[linkme::distributed_slice]
pub static STRUCTURE_POOL_ELEMENT_TYPES: [StructurePoolElementType];

/// `StructurePoolElement.CODEC`: `element_type` dispatch.
pub fn decode_element(
    value: &Value,
    ctx: &ElementCodecCtx<'_, '_>,
) -> LoadResult<Arc<dyn StructurePoolElement>> {
    let fields = Fields::new(value)?;
    let ty = fields.required("element_type", parse_identifier)?;
    let entry = (ty.namespace() == "minecraft")
        .then(|| {
            STRUCTURE_POOL_ELEMENT_TYPES
                .iter()
                .find(|e| e.name == ty.path())
        })
        .flatten();
    match entry {
        Some(entry) => (entry.decode)(&fields, ctx),
        None => Err(
            LoadError::new(format!("Unknown registry key in dispatch: {ty}")).at("element_type"),
        ),
    }
}

/// `Projection.CODEC` of the `projection` field.
pub fn parse_projection(value: &Value) -> LoadResult<Projection> {
    let name = parse_str(value)?;
    Projection::by_name(name).ok_or_else(|| LoadError::new(format!("Unknown element name:{name}")))
}

/// `StructurePoolElement.CODEC.parse(NbtOps, tag)`: the NBT is read through its JSON shape (strings, lists, compounds).
pub fn decode_element_from_nbt(
    tag: &CompoundTag,
    env: &GenEnv,
) -> LoadResult<Arc<dyn StructurePoolElement>> {
    decode_element(
        &tag_to_json(&Tag::Compound(tag.clone())),
        &ElementCodecCtx::Nbt(env),
    )
}

/// `NbtOps` -> `JsonOps` conversion of a tag.
pub fn tag_to_json(tag: &Tag) -> Value {
    match tag {
        Tag::Compound(c) => {
            Value::Object(c.iter().map(|(k, v)| (k.clone(), tag_to_json(v))).collect())
        }
        Tag::List(items) => Value::Array(items.iter().map(tag_to_json).collect()),
        Tag::String(s) => Value::String(s.clone()),
        Tag::Byte(v) => Value::from(*v),
        Tag::Short(v) => Value::from(*v),
        Tag::Int(v) => Value::from(*v),
        Tag::Long(v) => Value::from(*v),
        Tag::Float(v) => Number::from_f64(f64::from(*v)).map_or(Value::Null, Value::Number),
        Tag::Double(v) => Number::from_f64(*v).map_or(Value::Null, Value::Number),
        _ => Value::Null,
    }
}

/// `JsonOps` -> `NbtOps` conversion (`convertTo`): integers become `Int` (`Long` when out of range), fractions `Double`.
pub fn json_to_tag(value: &Value) -> Tag {
    match value {
        Value::Object(map) => {
            let mut tag = CompoundTag::new();
            for (k, v) in map {
                tag.put(k.clone(), json_to_tag(v));
            }
            Tag::Compound(tag)
        }
        Value::Array(items) => {
            let mut list = ListTag::new();
            for item in items {
                list.push(json_to_tag(item));
            }
            Tag::List(list)
        }
        Value::String(s) => Tag::String(s.clone()),
        Value::Bool(b) => Tag::Byte(i8::from(*b)),
        Value::Number(n) => match (n.as_i64(), n.as_f64()) {
            (Some(i), _) => i32::try_from(i).map_or(Tag::Long(i), Tag::Int),
            (None, Some(f)) => Tag::Double(f),
            _ => Tag::Int(0),
        },
        Value::Null => Tag::Compound(CompoundTag::new()),
    }
}
