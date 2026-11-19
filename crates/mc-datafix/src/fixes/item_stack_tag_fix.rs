//! `ItemStackTagFix`: rewrites the `tag` of the item stacks whose id passes `id_filter`. Abstract in Java;
//! `fix_item_stack_tag` is the subclass's `fixItemStackTag(Typed)`.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::Type;
use crate::typed::Typed;
use std::sync::Arc;

/// `new ItemStackTagFix(outputSchema, name, idFilter)` of the subclass `class`.
pub fn new(
    class: &str,
    output_schema: &Schema,
    name: &str,
    id_filter: impl Fn(&str) -> bool + Send + Sync + 'static,
    fix_item_stack_tag: impl Fn(Typed) -> Typed + Send + Sync + 'static,
) -> DataFix {
    let name = name.to_string();
    let (id_filter, fixer) = (Arc::new(id_filter), Arc::new(fix_item_stack_tag));
    DataFix::new(class, output_schema, false, move |df| {
        let item_stack_type = df.input_schema().get_type(&References::ITEM_STACK);
        let (id_filter, fixer) = (id_filter.clone(), fixer.clone());
        df.fix_type_everywhere_typed(
            &name,
            &item_stack_type,
            create_fixer(
                &item_stack_type,
                move |id| id_filter(id),
                move |tag| fixer(tag),
            ),
        )
    })
}

/// `ItemStackTagFix.createFixer(itemStackType, idFilter, fixer)`.
pub fn create_fixer(
    item_stack_type: &Type,
    id_filter: impl Fn(&str) -> bool + Send + Sync + 'static,
    fixer: impl Fn(Typed) -> Typed + Send + Sync + 'static,
) -> impl Fn(Typed) -> Typed + Send + Sync + 'static {
    let id_f = DSL::field_finder(
        "id",
        &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()),
    );
    let tag_f = item_stack_type.find_field("tag");
    move |input| match input.get_optional(&id_f) {
        Some(id) if id_filter(id.second().as_str()) => input.update_typed(&tag_f, &fixer),
        _ => input,
    }
}
