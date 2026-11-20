//! `RedstoneWireConnectionsFix`: fills in the `side` connection of redstone wire states that had none.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_core::java_string::JavaString;
use mc_dfu::Dynamic;

/// `new RedstoneWireConnectionsFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("RedstoneWireConnectionsFix", output_schema, false, |df| {
        df.fix_type_everywhere_typed(
            "RedstoneConnectionsFix",
            &df.input_schema().get_type(&References::BLOCK_STATE),
            |input| input.update_remainder(update_redstone_connections),
        )
    })
}

fn update_redstone_connections(state: Dynamic) -> Dynamic {
    let is_redstone =
        state.get("Name").as_string_lossy().result().as_deref() == Some("minecraft:redstone_wire");
    if !is_redstone {
        return state;
    }
    state.update("Properties", |props| {
        let east = props.get("east").as_string_or("none");
        let west = props.get("west").as_string_or("none");
        let north = props.get("north").as_string_or("none");
        let south = props.get("south").as_string_or("none");
        let eastwest = is_connected(east.as_lossy_str()) || is_connected(west.as_lossy_str());
        let northsouth = is_connected(north.as_lossy_str()) || is_connected(south.as_lossy_str());
        let side = |value: &JavaString, cross_connected| {
            value.map_lossless(|t| side_unless_connected(t, cross_connected).to_owned())
        };
        let new_east = side(&east, northsouth);
        let new_west = side(&west, northsouth);
        let new_north = side(&north, eastwest);
        let new_south = side(&south, eastwest);
        props
            .update("east", |value| value.create_string(new_east))
            .update("west", |value| value.create_string(new_west))
            .update("north", |value| value.create_string(new_north))
            .update("south", |value| value.create_string(new_south))
    })
}

/// `!isConnected(side) && !crossConnected ? "side" : side`.
fn side_unless_connected(side: &str, cross_connected: bool) -> &str {
    if !is_connected(side) && !cross_connected {
        "side"
    } else {
        side
    }
}

fn is_connected(connection_type: &str) -> bool {
    connection_type != "none"
}
