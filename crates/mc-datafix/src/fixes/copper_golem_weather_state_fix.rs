//! `CopperGolemWeatherStateFix`: converts the numeric `weather_state` of a copper golem to its name.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new CopperGolemWeatherStateFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    named_entity_fix::new(
        "CopperGolemWeatherStateFix",
        output_schema,
        false,
        "CopperGolemWeatherStateFix",
        &References::ENTITY,
        "minecraft:copper_golem",
        |entity| entity.update_remainder(|tag| tag.update("weather_state", fix_weather_state)),
    )
}

fn fix_weather_state(value: Dynamic) -> Dynamic {
    match value.as_int_or(0) {
        1 => value.create_string("exposed"),
        2 => value.create_string("weathered"),
        3 => value.create_string("oxidized"),
        _ => value.create_string("unaffected"),
    }
}
