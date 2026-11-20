//! `GossipUUIDFix`: converts the `Target` UUIDs of the gossips of one entity id to int arrays.

use crate::datafix::DataFix;
use crate::fixes::abstract_uuid_fix::replace_uuid_least_most;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new GossipUUIDFix(outputSchema, entityName)`.
pub fn new(schema: &Schema, entity_name: &str) -> DataFix {
    named_entity_fix::new(
        "GossipUUIDFix",
        schema,
        false,
        &format!("Gossip for for {entity_name}"),
        &References::ENTITY,
        entity_name,
        |entity| {
            entity.update_remainder(|tag| {
                tag.update("Gossips", |gossips| {
                    match gossips.as_stream_opt().result() {
                        Some(stream) => gossips.create_list(stream.into_iter().map(|gossip| {
                            replace_uuid_least_most(&gossip, "Target", "Target").unwrap_or(gossip)
                        })),
                        None => gossips,
                    }
                })
            })
        },
    )
}
