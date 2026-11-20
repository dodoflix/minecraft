//! `ForcedChunkToTicketFix`: turns the `Forced` chunk list of the tickets saved data into `minecraft:forced` tickets.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new ForcedChunkToTicketFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("ForcedChunkToTicketFix", output_schema, false, |df| {
        df.fix_type_everywhere_typed(
            "ForcedChunkToTicketFix",
            &df.input_schema().get_type(&References::SAVED_DATA_TICKETS),
            |input| {
                input.update_remainder(|remainder| {
                    remainder.update("data", |data| {
                        data.rename_and_fix_field("Forced", "tickets", |forced_chunks| {
                            forced_chunks.create_list(
                                forced_chunks.as_long_stream().into_iter().map(|l| {
                                    remainder
                                        .empty_map()
                                        .set("type", remainder.create_string("minecraft:forced"))
                                        .set("level", remainder.create_int(31))
                                        .set("ticks_left", remainder.create_long(0))
                                        .set("chunk_pos", remainder.create_long(l))
                                }),
                            )
                        })
                    })
                })
            },
        )
    })
}
