//! `ItemSpawnEggFix`: moves the legacy spawn egg damage value into `tag.EntityTag.id`.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::extra_data_fix_utils;
use crate::finder::remainder_finder;
use crate::fixes::references::References;
use crate::fixes::tables;
use crate::schema::Schema;
use std::sync::LazyLock;

static ID_TO_ENTITY: LazyLock<Vec<Option<String>>> = LazyLock::new(|| {
    tables::table("item_spawn_egg_fix")
        .field("ID_TO_ENTITY")
        .string_list()
});

/// `new ItemSpawnEggFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("ItemSpawnEggFix", output_schema, changes_type, |df| {
        let input_schema = df.input_schema();
        let item_stack_type = input_schema.get_type(&References::ITEM_STACK);
        let id_finder = DSL::field_finder(
            "id",
            &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()),
        );
        let entity_id_finder = DSL::field_finder("id", &DSL::string());
        let tag_finder = item_stack_type.find_field("tag");
        let entity_tag_finder = tag_finder.ty().find_field("EntityTag");
        let entity_finder = DSL::type_finder(&input_schema.get_type_raw(&References::ENTITY));
        df.fix_type_everywhere_typed("ItemSpawnEggFix", &item_stack_type, move |input| {
            let is_spawn_egg = input
                .get_optional(&id_finder)
                .is_some_and(|id| id.second().as_str() == "minecraft:spawn_egg");
            if !is_spawn_egg {
                return input;
            }
            let mut rest = input.get(&remainder_finder()).into_dynamic();
            let damage = rest.get("Damage").as_short_or(0);
            let old_id = input
                .get_optional_typed(&tag_finder)
                .and_then(|value| value.get_optional_typed(&entity_tag_finder))
                .and_then(|value| value.get_optional_typed(&entity_finder))
                .and_then(|value| value.get_optional(&entity_id_finder));
            let mut output = input.clone();
            if let Some(entity_name) = ID_TO_ENTITY[(damage & 255) as usize].as_deref()
                && old_id.is_none_or(|old| old.as_str() != entity_name)
            {
                let tag = input.get_or_create_typed(&tag_finder);
                let entity_tag = tag
                    .get_optional_typed(&entity_tag_finder)
                    .map(|entity_tree| entity_tree.write().get_or_throw())
                    .unwrap_or_else(|| rest.empty_map());
                let entity_tag = entity_tag.set("id", entity_tag.create_string(entity_name));
                output = input.set_typed(
                    &tag_finder,
                    extra_data_fix_utils::read_and_set(tag, &entity_tag_finder, &entity_tag),
                );
            }
            if damage != 0 {
                rest = rest.set("Damage", rest.create_short(0));
                output = output.set_remainder(rest);
            }
            output
        })
    })
}
