//! `NamespacedTypeRenameFix`: renames the ids of a namespaced-string type (`RECIPE`, `BIOME`, ...).

use crate::datafix::DataFix;
use crate::dsl::{DSL, TypeReference};
use crate::schema::Schema;
use crate::value::Value;

/// `new NamespacedTypeRenameFix(outputSchema, name, type, renamer)`.
pub fn new(
    output_schema: &Schema,
    name: &str,
    ty: &TypeReference,
    renamer: impl Fn(&str) -> String + Send + Sync + 'static,
) -> DataFix {
    let (name, ty) = (name.to_string(), *ty);
    let renamer = std::sync::Arc::new(renamer);
    DataFix::new("NamespacedTypeRenameFix", output_schema, false, move |df| {
        let field_type = DSL::named_type(ty.type_name(), &DSL::namespaced_string());
        if field_type != df.input_schema().get_type(&ty) {
            panic!(
                "java.lang.IllegalStateException: \"{}\" is not what was expected.",
                ty.type_name()
            );
        }
        let renamer = renamer.clone();
        df.fix_type_everywhere(&name, &field_type, move |_, input| {
            input.map_second(|id| Value::Str(id.as_java_string().map_text(|t| renamer(t))))
        })
    })
}
