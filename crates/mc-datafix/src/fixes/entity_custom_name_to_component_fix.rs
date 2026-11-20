//! `EntityCustomNameToComponentFix`: converts the plain string `CustomName` of entities to a text component.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::extra_data_fix_utils::cast;
use crate::fixes::references::References;
use crate::legacy_component_data_fix_utils::create_plain_text_component;
use crate::schema::Schema;
use crate::util::{read_typed_or_throw, write_and_read_typed_or_throw};
use mc_core::java_string::JavaString;
use mc_dfu::{Dynamic, Ops};

/// `new EntityCustomNameToComponentFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "EntityCustomNameToComponentFix",
        output_schema,
        true,
        |df| {
            let entity_type = df.input_schema().get_type(&References::ENTITY);
            let new_entity_type = df.output_schema().get_type(&References::ENTITY);
            let id_f = DSL::field_finder("id", &DSL::namespaced_string());
            let custom_name_f = entity_type.find_field("CustomName");
            let new_custom_name_type = new_entity_type.find_field_type("CustomName");
            df.fix_type_everywhere_typed_new(
                "EntityCustomNameToComponentFix",
                &entity_type,
                &new_entity_type.clone(),
                move |entity| {
                    let Some(custom_name) = entity.get_optional(&custom_name_f) else {
                        return cast(&new_entity_type, entity);
                    };
                    let custom_name = custom_name.into_string();
                    if custom_name.as_lossy_str().is_empty() {
                        write_and_read_typed_or_throw(&entity, &new_entity_type, |dynamic| {
                            dynamic.remove("CustomName")
                        })
                    } else {
                        let id = entity
                            .get_optional(&id_f)
                            .map(|id| id.as_lossy_str().to_string())
                            .unwrap_or_default();
                        let component = fix_custom_name(*entity.get_ops(), custom_name, &id);
                        entity.set_typed(
                            &custom_name_f,
                            read_typed_or_throw(&new_custom_name_type, &component, false),
                        )
                    }
                },
            )
        },
    )
}

fn fix_custom_name(ops: Ops, custom_name: JavaString, id: &str) -> Dynamic {
    if id == "minecraft:commandblock_minecart" {
        Dynamic::new(ops, ops.create_string(custom_name))
    } else {
        create_plain_text_component(ops, custom_name)
    }
}
