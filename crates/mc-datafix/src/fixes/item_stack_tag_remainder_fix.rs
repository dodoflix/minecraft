//! `ItemStackTagRemainderFix`: `ItemStackTagFix` over the remainder `Dynamic` of the `tag`. Abstract in Java;
//! `fix_item_stack_tag` is the subclass's `fixItemStackTag(Dynamic)`.

use crate::datafix::DataFix;
use crate::fixes::item_stack_tag_fix;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new ItemStackTagRemainderFix(outputSchema, name, idFilter)` of the subclass `class`.
pub fn new(
    class: &str,
    output_schema: &Schema,
    name: &str,
    id_filter: impl Fn(&str) -> bool + Send + Sync + 'static,
    fix_item_stack_tag: impl Fn(Dynamic) -> Dynamic + Send + Sync + 'static,
) -> DataFix {
    item_stack_tag_fix::new(class, output_schema, name, id_filter, move |tag| {
        tag.update_remainder(&fix_item_stack_tag)
    })
}
