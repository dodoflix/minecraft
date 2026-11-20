//! `AttributesRenameLegacy`: renames attribute names in item stack `AttributeModifiers`, entities and players.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use crate::typed::Typed;
use mc_dfu::Dynamic;
use std::sync::Arc;

type Renames = Arc<dyn Fn(&str) -> String + Send + Sync>;

/// `new AttributesRenameLegacy(outputSchema, name, renames)`.
pub fn new(
    output_schema: &Schema,
    name: &str,
    renames: impl Fn(&str) -> String + Send + Sync + 'static,
) -> DataFix {
    let name = name.to_string();
    let renames: Renames = Arc::new(renames);
    DataFix::new("AttributesRenameLegacy", output_schema, false, move |df| {
        let input_schema = df.input_schema();
        let item_stack_type = input_schema.get_type(&References::ITEM_STACK);
        let tag_f = item_stack_type.find_field("tag");
        let r = renames.clone();
        let item_stack = df.fix_type_everywhere_typed(
            &format!("{name} (ItemStack)"),
            &item_stack_type,
            move |item_stack| item_stack.update_typed(&tag_f, |tag| fix_item_stack_tag(&r, tag)),
        );
        let r = renames.clone();
        let entity = df.fix_type_everywhere_typed(
            &format!("{name} (Entity)"),
            &input_schema.get_type(&References::ENTITY),
            move |t| fix_entity(&r, t),
        );
        let r = renames.clone();
        let player = df.fix_type_everywhere_typed(
            &format!("{name} (Player)"),
            &input_schema.get_type(&References::PLAYER),
            move |t| fix_entity(&r, t),
        );
        TypeRewriteRule::seq_all(item_stack, vec![entity, player])
    })
}

fn fix_name(renames: &Renames, name: Dynamic) -> Dynamic {
    match name.as_string().result() {
        Some(s) => name.create_string(renames(&s)),
        None => name,
    }
}

/// `list.asStreamOpt().result().map(s -> s.map(fix)).map(list::createList)`, else `list`.
fn map_list(list: Dynamic, fix: impl Fn(Dynamic) -> Dynamic) -> Dynamic {
    match list.as_stream_opt().result() {
        Some(stream) => list.create_list(stream.into_iter().map(fix)),
        None => list,
    }
}

fn fix_item_stack_tag(renames: &Renames, item_stack: Typed) -> Typed {
    item_stack.update_remainder(|tag| {
        tag.update("AttributeModifiers", |modifiers| {
            map_list(modifiers, |modifier| {
                modifier.update("AttributeName", |n| fix_name(renames, n))
            })
        })
    })
}

fn fix_entity(renames: &Renames, entity: Typed) -> Typed {
    entity.update_remainder(|tag| {
        tag.update("Attributes", |attribute_list| {
            map_list(attribute_list, |attribute| {
                attribute.update("Name", |n| fix_name(renames, n))
            })
        })
    })
}
