//! `ItemStackUUIDFix`: converts the attribute modifier UUIDs and the player head `SkullOwner.Id` of item stacks to
//! int-array UUIDs.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::abstract_uuid_fix::{replace_uuid_least_most, replace_uuid_string};
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new ItemStackUUIDFix(outputSchema)`.
pub fn new(schema: &Schema) -> DataFix {
    DataFix::new("ItemStackUUIDFix", schema, false, |df| {
        let id_f = DSL::field_finder(
            "id",
            &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()),
        );
        df.fix_type_everywhere_typed(
            "ItemStackUUIDFix",
            &df.input_schema().get_type(&References::ITEM_STACK),
            move |input| {
                let item_tag_finder = input.get_type().find_field("tag");
                let is_player_head = input
                    .get_optional(&id_f)
                    .is_some_and(|id| id.second().as_str() == "minecraft:player_head");
                input.update_typed(&item_tag_finder, |typed_tag| {
                    typed_tag.update_remainder(|tag| {
                        let tag = update_attribute_modifiers(&tag);
                        if is_player_head {
                            update_skull_owner(&tag)
                        } else {
                            tag
                        }
                    })
                })
            },
        )
    })
}

fn update_attribute_modifiers(tag: &Dynamic) -> Dynamic {
    tag.update("AttributeModifiers", |modifiers| {
        tag.create_list(
            modifiers.as_stream().into_iter().map(|modifier| {
                replace_uuid_least_most(&modifier, "UUID", "UUID").unwrap_or(modifier)
            }),
        )
    })
}

fn update_skull_owner(tag: &Dynamic) -> Dynamic {
    tag.update("SkullOwner", |skull_owner| {
        replace_uuid_string(&skull_owner, "Id", "Id").unwrap_or(skull_owner)
    })
}
