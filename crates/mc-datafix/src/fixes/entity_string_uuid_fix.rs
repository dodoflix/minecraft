//! `EntityStringUuidFix`: converts the string `UUID` of the entities to `UUIDMost` / `UUIDLeast` longs.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_core::uuid_util::{least_significant_bits, most_significant_bits, uuid_from_string};

/// `new EntityStringUuidFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("EntityStringUuidFix", output_schema, changes_type, |df| {
        df.fix_type_everywhere_typed(
            "EntityStringUuidFix",
            &df.input_schema().get_type(&References::ENTITY),
            |input| {
                input.update_remainder(|tag| {
                    let uuid_string = tag.get("UUID").as_string().result();
                    let Some(uuid_string) = uuid_string else {
                        return tag;
                    };
                    let uuid = uuid_from_string(&uuid_string).unwrap_or_else(|message| {
                        // `UUID.fromString` throws `IllegalArgumentException`; its hex parse a `NumberFormatException`
                        let class = if message.starts_with("Invalid UUID string: ")
                            || message == "UUID string too large"
                        {
                            "IllegalArgumentException"
                        } else {
                            "NumberFormatException"
                        };
                        panic!("java.lang.{class}: {message}")
                    });
                    tag.remove("UUID")
                        .set("UUIDMost", tag.create_long(most_significant_bits(&uuid)))
                        .set("UUIDLeast", tag.create_long(least_significant_bits(&uuid)))
                })
            },
        )
    })
}
