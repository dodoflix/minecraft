//! `ItemStackEnchantmentNamesFix`: replaces the numeric enchantment ids of `ench` / `StoredEnchantments` with names.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::fixes::tables::{OrderedMap, table};
use crate::schema::Schema;
use mc_dfu::Dynamic;
use std::sync::LazyLock;

static MAP: LazyLock<OrderedMap<i32, String>> = LazyLock::new(|| {
    table("item_stack_enchantment_names_fix")
        .field("MAP")
        .int_string_map()
});

/// `new ItemStackEnchantmentNamesFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new(
        "ItemStackEnchantmentNamesFix",
        output_schema,
        changes_type,
        |df| {
            let item = df.input_schema().get_type(&References::ITEM_STACK);
            let tag_finder = item.find_field("tag");
            df.fix_type_everywhere_typed("ItemStackEnchantmentFix", &item, move |input| {
                input.update_typed(&tag_finder, |tag| tag.update_remainder(fix_tag))
            })
        },
    )
}

/// The enchantment with its `id` replaced by `MAP.getOrDefault(id, "null")`.
fn fix_id(element: Dynamic) -> Dynamic {
    let name = MAP
        .get(&element.get("id").as_int_or(0))
        .map_or("null", String::as_str);
    element.set("id", element.create_string(name))
}

/// `fixTag(tag)`.
fn fix_tag(tag: Dynamic) -> Dynamic {
    let new_ench = tag
        .get("ench")
        .as_stream_opt()
        .result()
        .map(|elements| tag.create_list(elements.into_iter().map(fix_id)));
    let tag = match new_ench {
        Some(ench) => tag.remove("ench").set("Enchantments", ench),
        None => tag,
    };
    tag.update("StoredEnchantments", |list| {
        match list.as_stream_opt().result() {
            Some(elements) => list.create_list(elements.into_iter().map(fix_id)),
            None => list,
        }
    })
}
