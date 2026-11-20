//! `AddFieldFix`: sets a generated field in the remainder of a type, optionally below a path of nested compounds
//! (a missing path element leaves the value unchanged).

use crate::datafix::DataFix;
use crate::dsl::TypeReference;
use crate::schema::Schema;
use mc_dfu::Dynamic;
use std::sync::Arc;

/// `new AddFieldFix(outputSchema, type, fieldName, fieldGenerator, path...)`.
pub fn new(
    output_schema: &Schema,
    ty: &TypeReference,
    field_name: &str,
    field_generator: impl Fn(&Dynamic) -> Dynamic + Send + Sync + 'static,
    path: &[&str],
) -> DataFix {
    let name = format!(
        "Adding field `{field_name}` to type `{}`",
        mc_core::java_case::to_lower_case_root(ty.type_name())
    );
    let ty = *ty;
    let field_name = field_name.to_string();
    let path: Arc<[String]> = path.iter().map(|p| p.to_string()).collect();
    let field_generator = Arc::new(field_generator);
    DataFix::new("AddFieldFix", output_schema, false, move |df| {
        let (field_name, path, field_generator) =
            (field_name.clone(), path.clone(), field_generator.clone());
        df.fix_type_everywhere_typed_new(
            &name,
            &df.input_schema().get_type(&ty),
            &df.output_schema().get_type(&ty),
            move |input| {
                input.update_remainder(|d| add_field(&d, 0, &field_name, &path, &*field_generator))
            },
        )
    })
}

fn add_field(
    dynamic: &Dynamic,
    path_index: usize,
    field_name: &str,
    path: &[String],
    field_generator: &dyn Fn(&Dynamic) -> Dynamic,
) -> Dynamic {
    if path_index >= path.len() {
        dynamic.set(field_name, field_generator(dynamic))
    } else {
        match dynamic.get(&path[path_index]).result() {
            None => dynamic.clone(),
            Some(field) => add_field(&field, path_index + 1, field_name, path, field_generator),
        }
    }
}
