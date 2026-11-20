//! `RemoveGolemGossipFix`: drops the `golem` gossips of villagers.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new RemoveGolemGossipFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    named_entity_fix::new(
        "RemoveGolemGossipFix",
        output_schema,
        changes_type,
        "Remove Golem Gossip Fix",
        &References::ENTITY,
        "minecraft:villager",
        |entity| entity.update_remainder(|tag| fix_value(&tag)),
    )
}

/// `RemoveGolemGossipFix.fixValue(tag)`.
fn fix_value(tag: &Dynamic) -> Dynamic {
    tag.update("Gossips", |gossips| {
        tag.create_list(
            gossips
                .as_stream()
                .into_iter()
                .filter(|attribute| attribute.get("Type").as_string_or("") != "golem"),
        )
    })
}
