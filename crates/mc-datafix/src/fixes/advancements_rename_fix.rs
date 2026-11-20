//! `AdvancementsRenameFix`: renames the advancement ids (the keys of `ADVANCEMENTS`).

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new AdvancementsRenameFix(outputSchema, changesType, name, renamer)` of the (sub)class `class`.
pub fn new(
    class: &str,
    output_schema: &Schema,
    changes_type: bool,
    name: &str,
    renamer: impl Fn(&str) -> String + Send + Sync + 'static,
) -> DataFix {
    let name = name.to_string();
    let renamer = std::sync::Arc::new(renamer);
    DataFix::new(class, output_schema, changes_type, move |df| {
        let renamer = renamer.clone();
        df.fix_type_everywhere_typed(
            &name,
            &df.input_schema().get_type(&References::ADVANCEMENTS),
            move |input| {
                input.update_remainder(|tag| {
                    tag.update_map_values(|(key, value)| {
                        let id = key.as_string().result().unwrap_or_default();
                        (tag.create_string(renamer(id.as_lossy_str())), value)
                    })
                })
            },
        )
    })
}
