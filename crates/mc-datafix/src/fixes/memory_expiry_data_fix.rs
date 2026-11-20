//! `MemoryExpiryDataFix`: wraps every brain memory of an entity in a `{ value: .. }` compound.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new MemoryExpiryDataFix(schema, entityType)`.
pub fn new(schema: &Schema, entity_name: &str) -> DataFix {
    named_entity_fix::new(
        "MemoryExpiryDataFix",
        schema,
        false,
        &format!("Memory expiry data fix ({entity_name})"),
        &References::ENTITY,
        entity_name,
        |entity| entity.update_remainder(fix_tag),
    )
}

/// `fixTag(input)`.
pub fn fix_tag(input: Dynamic) -> Dynamic {
    input.update("Brain", |brain| {
        brain.update("memories", |memories| {
            memories.update_map_values(|(key, value)| {
                let wrapped = value.create_map([(value.create_string("value"), value.clone())]);
                (key, wrapped)
            })
        })
    })
}
