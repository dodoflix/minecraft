//! `DayTimeToClockFix`: the level's day time becomes the total ticks of an overworld world clock.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new DayTimeToClockFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("DayTimeToClockFix", output_schema, false, |df| {
        df.fix_type_everywhere_typed(
            "DayTimeToClockFix",
            &df.input_schema().get_type(&References::LEVEL),
            |typed| {
                typed.update_remainder(|input| {
                    let game_time = input.get("Time").as_long_or(0);
                    let day_time = input.get("DayTime").as_long_or(game_time);
                    let input = input.remove("DayTime");
                    let overworld_clock = create_clock(&input, day_time);
                    input.set(
                        "world_clocks",
                        input
                            .empty_map()
                            .set("minecraft:overworld", overworld_clock),
                    )
                })
            },
        )
    })
}

fn create_clock(input: &Dynamic, total_ticks: i64) -> Dynamic {
    input
        .empty_map()
        .set("total_ticks", input.create_long(total_ticks))
        .set("paused", input.create_boolean(false))
}
