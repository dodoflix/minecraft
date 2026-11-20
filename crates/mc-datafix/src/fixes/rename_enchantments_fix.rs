//! `RenameEnchantmentsFix`: renames enchantment ids of item stacks.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::ensure_namespaced;
use mc_core::java_string::JavaString;
use mc_dfu::Dynamic;
use std::collections::HashMap;
use std::sync::Arc;

/// `new RenameEnchantmentsFix(outputSchema, name, renames)`.
pub fn new(output_schema: &Schema, name: &str, renames: &[(&str, &str)]) -> DataFix {
    let name = name.to_string();
    let renames: Arc<HashMap<String, String>> = Arc::new(
        renames
            .iter()
            .map(|(from, to)| (from.to_string(), to.to_string()))
            .collect(),
    );
    DataFix::new("RenameEnchantmentsFix", output_schema, false, move |df| {
        let item = df.input_schema().get_type(&References::ITEM_STACK);
        let tag_finder = item.find_field("tag");
        let renames = renames.clone();
        df.fix_type_everywhere_typed(&name, &item, move |input| {
            input.update_typed(&tag_finder, |tag| {
                tag.update_remainder(|tag| fix_tag(&renames, tag))
            })
        })
    })
}

fn fix_tag(renames: &HashMap<String, String>, tag: Dynamic) -> Dynamic {
    let tag = fix_enchantment_list(renames, tag, "Enchantments");
    fix_enchantment_list(renames, tag, "StoredEnchantments")
}

fn fix_enchantment_list(
    renames: &HashMap<String, String>,
    item_stack: Dynamic,
    field: &str,
) -> Dynamic {
    item_stack.update(field, |tag| match tag.as_stream_opt().result() {
        Some(stream) => {
            let elements: Vec<Dynamic> = stream
                .into_iter()
                .map(|element| {
                    let source = element.clone();
                    element.update("id", |id| match id.as_string().result() {
                        Some(string_id) => {
                            let renamed =
                                match renames.get(&ensure_namespaced(string_id.as_lossy_str())) {
                                    Some(renamed) => JavaString::from(renamed.as_str()),
                                    None => string_id,
                                };
                            source.create_string(renamed)
                        }
                        None => id,
                    })
                })
                .collect();
            tag.create_list(elements)
        }
        None => tag,
    })
}
