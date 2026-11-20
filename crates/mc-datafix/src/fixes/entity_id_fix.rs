//! `EntityIdFix`: renames the legacy entity ids to namespaced ids (the `ENTITY` choice changes).

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::fixes::tables::{OrderedMap, table};
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use crate::value::Value;
use std::sync::LazyLock;

static ID_MAP: LazyLock<OrderedMap<String, String>> =
    LazyLock::new(|| table("entity_id_fix").field("ID_MAP").string_map());

/// `new EntityIdFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("EntityIdFix", output_schema, changes_type, |df| {
        let old_type = df.input_schema().find_choice_type(&References::ENTITY);
        let new_type = df.output_schema().find_choice_type(&References::ENTITY);
        let old_item_stack_type = df.input_schema().get_type(&References::ITEM_STACK);
        let new_item_stack_type = df.output_schema().get_type(&References::ITEM_STACK);
        TypeRewriteRule::seq(
            df.convert_unchecked(
                "item stack entity name hook converter",
                &old_item_stack_type,
                &new_item_stack_type,
            ),
            df.fix_type_everywhere_new("EntityIdFix", old_type.ty(), new_type.ty(), |_, input| {
                input.map_first(|id| {
                    let id = id.into_string();
                    Value::Str(
                        id.map_lossless(|t| ID_MAP.get(t).cloned().unwrap_or_else(|| t.to_owned())),
                    )
                })
            }),
        )
    })
}
