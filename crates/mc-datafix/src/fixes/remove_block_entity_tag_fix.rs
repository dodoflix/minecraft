//! `RemoveBlockEntityTagFix`: drops the block entity data of the given block entity ids from items, falling blocks,
//! structures and chunks.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::extra_data_fix_utils::cast_value;
use crate::finder::OpticFinder;
use crate::fixes::references::References;
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use crate::typed::Typed;
use crate::util::write_and_read_typed_or_throw;
use crate::value::Value;
use std::collections::HashSet;
use std::sync::Arc;

/// `new RemoveBlockEntityTagFix(outputSchema, blockEntityIdsToDrop)`.
pub fn new(output_schema: &Schema, block_entity_ids_to_drop: &[&str]) -> DataFix {
    new_with_legacy_data_structure(output_schema, false, block_entity_ids_to_drop)
}

/// `new RemoveBlockEntityTagFix(outputSchema, useLegacyDataStructure, blockEntityIdsToDrop)`.
pub fn new_with_legacy_data_structure(
    output_schema: &Schema,
    use_legacy_data_structure: bool,
    block_entity_ids_to_drop: &[&str],
) -> DataFix {
    let ids: Arc<HashSet<String>> = Arc::new(
        block_entity_ids_to_drop
            .iter()
            .map(|id| (*id).to_string())
            .collect(),
    );
    DataFix::new("RemoveBlockEntityTagFix", output_schema, true, move |df| {
        let block_entity_id_f = DSL::field_finder("id", &DSL::namespaced_string());
        if use_legacy_data_structure {
            TypeRewriteRule::seq_list(vec![
                create_item_block_entity_remover(
                    df,
                    &ids,
                    &block_entity_id_f,
                    "tag",
                    "BlockEntityTag",
                ),
                create_falling_block_block_entity_remover(df, &ids, &block_entity_id_f),
                create_structure_block_entity_remover(df, &ids, &block_entity_id_f),
                create_unchecked_converter_hack(df),
            ])
        } else {
            TypeRewriteRule::seq_list(vec![
                create_item_block_entity_remover(
                    df,
                    &ids,
                    &block_entity_id_f,
                    "components",
                    "minecraft:block_entity_data",
                ),
                create_falling_block_block_entity_remover(df, &ids, &block_entity_id_f),
                create_structure_block_entity_remover(df, &ids, &block_entity_id_f),
                create_chunk_block_entity_remover(df, &ids, &block_entity_id_f),
                create_unchecked_converter_hack(df),
            ])
        }
    })
}

fn create_item_block_entity_remover(
    df: &DataFix,
    ids: &Arc<HashSet<String>>,
    block_entity_id_f: &OpticFinder,
    item_tag_or_component_key: &str,
    item_block_entity_data_key: &str,
) -> TypeRewriteRule {
    let item_stack_type = df.input_schema().get_type(&References::ITEM_STACK);
    let item_tag_f = item_stack_type.find_field(item_tag_or_component_key);
    let item_block_entity_f = item_tag_f.ty().find_field(item_block_entity_data_key);
    let (ids, block_entity_id_f, key) = (
        ids.clone(),
        block_entity_id_f.clone(),
        item_block_entity_data_key.to_string(),
    );
    df.fix_type_everywhere_typed(
        &format!("ItemRemoveBlockEntityTagFix{}", df.version_key()),
        &item_stack_type,
        move |input| {
            input.update_typed(&item_tag_f, |tag| {
                remove_block_entity(&ids, tag, &item_block_entity_f, &block_entity_id_f, &key)
            })
        },
    )
}

fn create_falling_block_block_entity_remover(
    df: &DataFix,
    ids: &Arc<HashSet<String>>,
    block_entity_id_f: &OpticFinder,
) -> TypeRewriteRule {
    let entity_type = df.input_schema().get_type(&References::ENTITY);
    let falling_block_f = DSL::named_choice(
        "minecraft:falling_block",
        &df.input_schema()
            .get_choice_type(&References::ENTITY, "minecraft:falling_block"),
    );
    let falling_block_entity_tag_f = falling_block_f.ty().find_field("TileEntityData");
    let (ids, block_entity_id_f) = (ids.clone(), block_entity_id_f.clone());
    df.fix_type_everywhere_typed(
        &format!(
            "FallingBlockEntityRemoveBlockEntityTagFix{}",
            df.version_key()
        ),
        &entity_type,
        move |input| {
            input.update_typed(&falling_block_f, |tag| {
                remove_block_entity(
                    &ids,
                    tag,
                    &falling_block_entity_tag_f,
                    &block_entity_id_f,
                    "TileEntityData",
                )
            })
        },
    )
}

fn create_structure_block_entity_remover(
    df: &DataFix,
    ids: &Arc<HashSet<String>>,
    block_entity_id_f: &OpticFinder,
) -> TypeRewriteRule {
    let structure_type = df.input_schema().get_type(&References::STRUCTURE);
    let blocks_f = structure_type.find_field("blocks");
    let block_type = blocks_f
        .ty()
        .as_list()
        .expect("the blocks of a structure are a list")
        .clone();
    let block_type_f = DSL::type_finder(&block_type);
    let block_nbt_f = block_type_f.ty().find_field("nbt");
    let (ids, block_entity_id_f) = (ids.clone(), block_entity_id_f.clone());
    df.fix_type_everywhere_typed(
        &format!("StructureRemoveBlockEntityTagFix{}", df.version_key()),
        &structure_type,
        move |input| {
            input.update_typed(&blocks_f, |tag| {
                tag.update_typed(&block_type_f, |block_tag| {
                    remove_block_entity(&ids, block_tag, &block_nbt_f, &block_entity_id_f, "nbt")
                })
            })
        },
    )
}

fn create_chunk_block_entity_remover(
    df: &DataFix,
    ids: &Arc<HashSet<String>>,
    block_entity_id_f: &OpticFinder,
) -> TypeRewriteRule {
    let chunk_type = df.input_schema().get_type(&References::CHUNK);
    let block_entities_f = chunk_type.find_field("block_entities");
    let block_entity_elements_type = block_entities_f
        .ty()
        .as_list()
        .expect("the block entities of a chunk are a list")
        .clone();
    let block_entity_type_finder = df
        .input_schema()
        .get_type(&References::BLOCK_ENTITY)
        .finder();
    let chunk_type_out = df.output_schema().get_type(&References::CHUNK);
    let block_entities_type_out = chunk_type.find_field("block_entities").ty().clone();
    let (ids, block_entity_id_f) = (ids.clone(), block_entity_id_f.clone());
    df.fix_type_everywhere_typed_new(
        &format!("BlockEntityChunkRemover{}", df.version_key()),
        &chunk_type,
        &chunk_type_out,
        move |input| {
            let ops = input.ops;
            input.update_as(&block_entities_f, &block_entities_type_out, |list_tag| {
                let mut kept_block_entities: Vec<Value> = Vec::new();
                for untyped_block_entity in list_tag.into_list() {
                    let typed_block_entity = cast_value(
                        &block_entity_elements_type,
                        untyped_block_entity.clone(),
                        ops,
                    );
                    let typed_block_entity_unwrapped =
                        typed_block_entity.get_or_create_typed(&block_entity_type_finder);
                    let block_entity_id = typed_block_entity_unwrapped
                        .get_optional(&block_entity_id_f)
                        .map(|id| id.as_lossy_str().to_string())
                        .unwrap_or_default();
                    if !ids.contains(&block_entity_id) {
                        kept_block_entities.push(untyped_block_entity);
                    }
                }
                Value::List(kept_block_entities)
            })
        },
    )
}

fn create_unchecked_converter_hack(df: &DataFix) -> TypeRewriteRule {
    df.convert_unchecked(
        &format!(
            "ItemRemoveBlockEntityTagFix - update block entity type{}",
            df.version_key()
        ),
        &df.input_schema().get_type(&References::BLOCK_ENTITY),
        &df.output_schema().get_type(&References::BLOCK_ENTITY),
    )
}

fn remove_block_entity(
    ids: &HashSet<String>,
    tag: Typed,
    block_entity_f: &OpticFinder,
    block_entity_id_f: &OpticFinder,
    block_entity_field_name: &str,
) -> Typed {
    let Some(block_entity) = tag.get_optional_typed(block_entity_f) else {
        return tag;
    };
    let block_entity_id = block_entity
        .get_optional(block_entity_id_f)
        .map(|id| id.as_lossy_str().to_string())
        .unwrap_or_default();
    if !ids.contains(&block_entity_id) {
        tag
    } else {
        write_and_read_typed_or_throw(&tag, &tag.ty, |tag_data| {
            tag_data.remove(block_entity_field_name)
        })
    }
}
