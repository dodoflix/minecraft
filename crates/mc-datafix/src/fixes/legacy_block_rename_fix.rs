//! `LegacyBlockRenameFix`: renames blocks in `BLOCK_NAME`, `BLOCK_STATE` and `FLAT_BLOCK_STATE`. Abstract in Java
//! (`rename_block` is the subclass's `renameBlock`); [`create`] is the `LegacyBlockRenameFix.create` factory.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use crate::value::Value;
use mc_dfu::Dynamic;
use std::sync::Arc;

type Renamer = Arc<dyn Fn(&str) -> String + Send + Sync>;

/// `new LegacyBlockRenameFix(outputSchema, name)` of the subclass `class`.
pub fn new(
    class: &str,
    output_schema: &Schema,
    name: &str,
    rename_block: impl Fn(&str) -> String + Send + Sync + 'static,
) -> DataFix {
    let name = name.to_string();
    let rename_block: Renamer = Arc::new(rename_block);
    DataFix::new(class, output_schema, false, move |df| {
        let input_schema = df.input_schema();
        let block_type = input_schema.get_type(&References::BLOCK_NAME);
        let expected_type = DSL::named_type(
            References::BLOCK_NAME.type_name(),
            &DSL::namespaced_string(),
        );
        if block_type != expected_type {
            panic!("java.lang.IllegalStateException: block type is not what was expected.");
        }
        let r = rename_block.clone();
        let block_rule = df.fix_type_everywhere(
            &format!("{name} for block"),
            &expected_type,
            move |_, input| input.map_second(|b| Value::Str(b.as_java_string().map_text(|t| r(t)))),
        );
        let r = rename_block.clone();
        let block_state_rule = df.fix_type_everywhere_typed(
            &format!("{name} for block_state"),
            &input_schema.get_type(&References::BLOCK_STATE),
            move |input| input.update_remainder(|tag| fix_block_state(&r, tag)),
        );
        let r = rename_block.clone();
        let flat_block_state_rule = df.fix_type_everywhere_typed(
            &format!("{name} for flat_block_state"),
            &input_schema.get_type(&References::FLAT_BLOCK_STATE),
            move |input| {
                input.update_remainder(|tag| match tag.as_string().result() {
                    Some(s) => tag.create_string(fix_flat_block_state(&r, s.as_lossy_str())),
                    None => tag,
                })
            },
        );
        TypeRewriteRule::seq_all(block_rule, vec![block_state_rule, flat_block_state_rule])
    })
}

fn fix_block_state(rename_block: &Renamer, tag: Dynamic) -> Dynamic {
    match tag.get("Name").as_string().result() {
        Some(name) => tag.set("Name", tag.create_string(rename_block(name.as_lossy_str()))),
        None => tag,
    }
}

fn fix_flat_block_state(rename_block: &Renamer, string: &str) -> String {
    let start_properties = string.find('[');
    let start_nbt = string.find('{');
    let mut end = string.len();
    if let Some(p) = start_properties.filter(|&p| p > 0) {
        end = p;
    }
    if let Some(n) = start_nbt.filter(|&n| n > 0) {
        end = end.min(n);
    }
    let new_name = rename_block(&string[..end]);
    format!("{new_name}{}", &string[end..])
}

/// `LegacyBlockRenameFix.create(outputSchema, name, renamer)`.
pub fn create(
    output_schema: &Schema,
    name: &str,
    renamer: impl Fn(&str) -> String + Send + Sync + 'static,
) -> DataFix {
    new("LegacyBlockRenameFix$1", output_schema, name, renamer)
}
