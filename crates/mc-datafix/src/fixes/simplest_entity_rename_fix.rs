//! `SimplestEntityRenameFix`: renames the entity ids (the `ENTITY` choice and the `ENTITY_NAME` type), nothing else.
//! Abstract in Java; `rename` is the subclass's `rename(name)`.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::entity_rename_fix::null_pointer;
use crate::fixes::references::References;
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use crate::value::Value;
use std::sync::Arc;

/// `new SimplestEntityRenameFix(name, outputSchema, changesType)` of the subclass `class`.
pub fn new(
    class: &str,
    name: &str,
    output_schema: &Schema,
    changes_type: bool,
    rename: impl Fn(&str) -> String + Send + Sync + 'static,
) -> DataFix {
    let name = name.to_string();
    let rename = Arc::new(rename);
    DataFix::new(class, output_schema, changes_type, move |df| {
        let old_type = df.input_schema().find_choice_type(&References::ENTITY);
        let new_type = df.output_schema().find_choice_type(&References::ENTITY);
        let entity_name_type = DSL::named_type(
            References::ENTITY_NAME.type_name(),
            &DSL::namespaced_string(),
        );
        if df.output_schema().get_type(&References::ENTITY_NAME) != entity_name_type {
            panic!("java.lang.IllegalStateException: Entity name type is not what was expected.");
        }
        let (old_type2, new_type2, rename2) = (old_type.clone(), new_type.clone(), rename.clone());
        let entities = df.fix_type_everywhere_new(&name, old_type.ty(), new_type.ty(), move |_, input| {
            input.map_first(|old_name| {
                let old_name = old_name.into_string();
                let new_name = rename2(&old_name);
                let old_entity_type = old_type2.get(&old_name);
                let new_entity_type = new_type2
                    .get(&new_name)
                    .unwrap_or_else(|| panic!("{}", null_pointer("newEntityType")));
                if !old_entity_type.is_some_and(|t| new_entity_type.equals_with(t, true, true)) {
                    panic!(
                        "java.lang.IllegalStateException: Dynamic type check failed: {new_entity_type} not equal to {}",
                        old_entity_type.map_or("null".to_string(), |t| t.to_string())
                    );
                }
                Value::Str(new_name)
            })
        });
        let rename = rename.clone();
        let names = df.fix_type_everywhere(
            &format!("{name} for entity name"),
            &entity_name_type,
            move |_, input| input.map_second(|n| Value::Str(rename(n.as_str()))),
        );
        TypeRewriteRule::seq(entities, names)
    })
}
