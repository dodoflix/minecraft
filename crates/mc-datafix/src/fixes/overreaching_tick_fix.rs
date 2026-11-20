//! `OverreachingTickFix`: moves ticks saved in the wrong chunk into `UpgradeData`.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new OverreachingTickFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("OverreachingTickFix", output_schema, false, |df| {
        let chunk_type = df.input_schema().get_type(&References::CHUNK);
        let block_ticks_finder = chunk_type.find_field("block_ticks");
        df.fix_type_everywhere_typed(
            "Handle ticks saved in the wrong chunk",
            &chunk_type,
            move |chunk| {
                let block_ticks = chunk
                    .get_optional_typed(&block_ticks_finder)
                    .and_then(|ticks| ticks.write().result());
                chunk.update_remainder(|remainder| {
                    let chunk_x = remainder.get("xPos").as_int_or(0);
                    let chunk_z = remainder.get("zPos").as_int_or(0);
                    let fluid_ticks = remainder.get("fluid_ticks").result();
                    let remainder = extract_overreaching_ticks(
                        remainder,
                        chunk_x,
                        chunk_z,
                        block_ticks.as_ref(),
                        "neighbor_block_ticks",
                    );
                    extract_overreaching_ticks(
                        remainder,
                        chunk_x,
                        chunk_z,
                        fluid_ticks.as_ref(),
                        "neighbor_fluid_ticks",
                    )
                })
            },
        )
    })
}

fn extract_overreaching_ticks(
    remainder: Dynamic,
    chunk_x: i32,
    chunk_z: i32,
    ticks: Option<&Dynamic>,
    name_in_upgrade_data: &str,
) -> Dynamic {
    let Some(ticks) = ticks else {
        return remainder;
    };
    let overreaching_ticks: Vec<Dynamic> = ticks
        .as_stream()
        .into_iter()
        .filter(|tick| {
            let x = tick.get("x").as_int_or(0);
            let z = tick.get("z").as_int_or(0);
            let dist_x = chunk_x.wrapping_sub(x >> 4).wrapping_abs();
            let dist_z = chunk_z.wrapping_sub(z >> 4).wrapping_abs();
            (dist_x != 0 || dist_z != 0) && dist_x <= 1 && dist_z <= 1
        })
        .collect();
    if overreaching_ticks.is_empty() {
        return remainder;
    }
    let upgrade_data = remainder.get("UpgradeData").or_else_empty_map().set(
        name_in_upgrade_data,
        remainder.create_list(overreaching_ticks),
    );
    remainder.set("UpgradeData", upgrade_data)
}
