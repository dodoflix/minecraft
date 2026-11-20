//! `RemoveEmptyItemInBrushableBlockFix`: drops an air or empty item from a brushable block.

use crate::datafix::DataFix;
use crate::fixes::named_entity_write_read_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::ensure_namespaced;
use mc_dfu::Dynamic;

/// `new RemoveEmptyItemInBrushableBlockFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    named_entity_write_read_fix::new(
        "RemoveEmptyItemInBrushableBlockFix",
        output_schema,
        false,
        "RemoveEmptyItemInSuspiciousBlockFix",
        &References::BLOCK_ENTITY,
        "minecraft:brushable_block",
        |input| match input.get("item").result() {
            Some(item) if is_empty_stack(&item) => input.remove("item"),
            _ => input,
        },
    )
}

fn is_empty_stack(item: &Dynamic) -> bool {
    let id = ensure_namespaced(&item.get("id").as_string_or("minecraft:air"));
    let count = item.get("count").as_int_or(0);
    id == "minecraft:air" || count == 0
}
