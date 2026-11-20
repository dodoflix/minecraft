//! `ChunkTicketUnpackPosFix`: unpacks the packed long `chunk_pos` of every ticket into an `[x, z]` int list.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

const CHUNK_COORD_BITS: u32 = 32;
const CHUNK_COORD_MASK: i64 = 4294967295;

/// `new ChunkTicketUnpackPosFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("ChunkTicketUnpackPosFix", output_schema, false, |df| {
        df.fix_type_everywhere_typed(
            "ChunkTicketUnpackPosFix",
            &df.input_schema().get_type(&References::SAVED_DATA_TICKETS),
            |input| {
                input.update_remainder(|remainder| {
                    remainder.update("data", |data| {
                        data.update("tickets", |tickets| {
                            tickets.create_list(tickets.as_stream().into_iter().map(|ticket| {
                                ticket.update("chunk_pos", |chunk_pos| {
                                    let key = chunk_pos.as_long_or(0);
                                    let x = (key & CHUNK_COORD_MASK) as i32;
                                    let z = (((key as u64) >> CHUNK_COORD_BITS) as i64
                                        & CHUNK_COORD_MASK)
                                        as i32;
                                    chunk_pos.create_int_list([x, z])
                                })
                            }))
                        })
                    })
                })
            },
        )
    })
}
