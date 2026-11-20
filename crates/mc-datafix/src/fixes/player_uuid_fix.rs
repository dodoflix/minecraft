//! `PlayerUUIDFix`: converts the `RootVehicle.Attach`, the attribute modifier and the player `UUID` to int-array UUIDs.

use crate::datafix::DataFix;
use crate::fixes::abstract_uuid_fix::replace_uuid_least_most;
use crate::fixes::entity_uuid_fix::{update_entity_uuid, update_living_entity};
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new PlayerUUIDFix(outputSchema)`.
pub fn new(schema: &Schema) -> DataFix {
    DataFix::new("PlayerUUIDFix", schema, false, |df| {
        df.fix_type_everywhere_typed(
            "PlayerUUIDFix",
            &df.input_schema().get_type(&References::PLAYER),
            |input| {
                let root_vehicle_finder = input.get_type().find_field("RootVehicle");
                input
                    .update_typed_as(
                        &root_vehicle_finder,
                        root_vehicle_finder.ty(),
                        |root_vehicle| {
                            root_vehicle.update_remainder(|tag| {
                                replace_uuid_least_most(&tag, "Attach", "Attach").unwrap_or(tag)
                            })
                        },
                    )
                    .update_remainder(|tag| update_entity_uuid(&update_living_entity(&tag)))
            },
        )
    })
}
