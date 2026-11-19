//! `AttributesRenameFix`: renames attribute ids in item components, entities and players.

use crate::datafix::DataFix;
use crate::extra_data_fix_utils::fix_string_field;
use crate::fixes::references::References;
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use crate::typed::Typed;
use mc_dfu::Dynamic;
use std::sync::Arc;

type Renames = Arc<dyn Fn(&str) -> String + Send + Sync>;

/// `new AttributesRenameFix(outputSchema, name, renames)` of the subclass `class`.
pub fn new(
    class: &str,
    output_schema: &Schema,
    name: &str,
    renames: impl Fn(&str) -> String + Send + Sync + 'static,
) -> DataFix {
    new_with_format(class, output_schema, name, renames, false)
}

/// `new AttributesRenameFix(outputSchema, name, renames, oldDataComponentFormat)` of the subclass `class`.
pub fn new_with_format(
    class: &str,
    output_schema: &Schema,
    name: &str,
    renames: impl Fn(&str) -> String + Send + Sync + 'static,
    old_data_component_format: bool,
) -> DataFix {
    let name = name.to_string();
    let renames: Renames = Arc::new(renames);
    DataFix::new(class, output_schema, false, move |df| {
        let input_schema = df.input_schema();
        let r = renames.clone();
        let components = df.fix_type_everywhere_typed(
            &format!("{name} (Components)"),
            &input_schema.get_type(&References::DATA_COMPONENTS),
            move |t| {
                if old_data_component_format {
                    fix_data_components_old(&r, t)
                } else {
                    fix_data_components(&r, t)
                }
            },
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
        TypeRewriteRule::seq_all(components, vec![entity, player])
    })
}

/// `modifiers.asStreamOpt().result().map(stream -> stream.map(fix)).map(modifiers::createList)`, else `modifiers`.
fn map_list(list: Dynamic, fix: impl Fn(Dynamic) -> Dynamic) -> Dynamic {
    match list.as_stream_opt().result() {
        Some(stream) => list.create_list(stream.into_iter().map(fix)),
        None => list,
    }
}

fn fix_data_components(renames: &Renames, components: Typed) -> Typed {
    components.update_remainder(|component_data| {
        component_data.update("minecraft:attribute_modifiers", |modifiers| {
            map_list(modifiers, |d| fix_type_field(renames, &d))
        })
    })
}

fn fix_data_components_old(renames: &Renames, components: Typed) -> Typed {
    components.update_remainder(|component_data| {
        component_data.update("minecraft:attribute_modifiers", |attribute_modifiers| {
            attribute_modifiers.update("modifiers", |modifiers| {
                map_list(modifiers, |d| fix_type_field(renames, &d))
            })
        })
    })
}

fn fix_entity(renames: &Renames, entity: Typed) -> Typed {
    entity.update_remainder(|tag| {
        tag.update("attributes", |list| {
            map_list(list, |d| fix_string_field(&d, "id", |s| renames(s)))
        })
    })
}

fn fix_type_field(renames: &Renames, dynamic: &Dynamic) -> Dynamic {
    fix_string_field(dynamic, "type", |s| renames(s))
}
