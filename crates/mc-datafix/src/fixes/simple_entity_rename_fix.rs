//! `SimpleEntityRenameFix`: `EntityRenameFix` over the remainder `Dynamic`. Abstract in Java;
//! `get_new_name_and_tag` is the subclass's `getNewNameAndTag(name, tag)`.

use crate::datafix::DataFix;
use crate::fixes::entity_rename_fix;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new SimpleEntityRenameFix(name, outputSchema, changesType)` of the subclass `class`.
pub fn new(
    class: &str,
    name: &str,
    output_schema: &Schema,
    changes_type: bool,
    get_new_name_and_tag: impl Fn(&str, Dynamic) -> (String, Dynamic) + Send + Sync + 'static,
) -> DataFix {
    entity_rename_fix::new(
        class,
        name,
        output_schema,
        changes_type,
        move |name, entity| {
            let (new_name, tag) = get_new_name_and_tag(name, entity.get_remainder());
            (new_name, entity.set_remainder(tag))
        },
    )
}
