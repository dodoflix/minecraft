//! `BlockEntityUUIDFix`: converts the conduit `target_uuid` and the skull owner id to int-array UUIDs.

use crate::datafix::DataFix;
use crate::fixes::abstract_uuid_fix::{
    replace_uuid_ml_tag, replace_uuid_string, update_named_choice,
};
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new BlockEntityUUIDFix(outputSchema)`.
pub fn new(schema: &Schema) -> DataFix {
    DataFix::new("BlockEntityUUIDFix", schema, false, |df| {
        let schema = df.input_schema();
        df.fix_type_everywhere_typed(
            "BlockEntityUUIDFix",
            &schema.get_type(&References::BLOCK_ENTITY),
            move |input| {
                let input = update_named_choice(
                    &schema,
                    &References::BLOCK_ENTITY,
                    input,
                    "minecraft:conduit",
                    |tag| update_conduit(&tag),
                );
                update_named_choice(
                    &schema,
                    &References::BLOCK_ENTITY,
                    input,
                    "minecraft:skull",
                    |tag| update_skull(&tag),
                )
            },
        )
    })
}

fn update_skull(tag: &Dynamic) -> Dynamic {
    match tag.get("Owner").result() {
        Some(owner_tag) => {
            let owner_tag = replace_uuid_string(&owner_tag, "Id", "Id").unwrap_or(owner_tag);
            tag.remove("Owner").set("SkullOwner", owner_tag)
        }
        None => tag.clone(),
    }
}

fn update_conduit(tag: &Dynamic) -> Dynamic {
    replace_uuid_ml_tag(tag, "target_uuid", "Target").unwrap_or_else(|| tag.clone())
}
