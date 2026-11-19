//! `AbstractPoiSectionFix`: rewrites the `Records` of every section of a `POI_CHUNK`. Abstract in Java;
//! `process_records` is the subclass's `processRecords(Stream)`.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::value::Value;
use mc_dfu::Dynamic;
use std::sync::Arc;

/// `new AbstractPoiSectionFix(outputSchema, name)` of the subclass `class`.
pub fn new(
    class: &str,
    output_schema: &Schema,
    name: &str,
    process_records: impl Fn(Vec<Dynamic>) -> Vec<Dynamic> + Send + Sync + 'static,
) -> DataFix {
    let name = name.to_string();
    let process_records = Arc::new(process_records);
    DataFix::new(class, output_schema, false, move |df| {
        let poi_chunk_type =
            DSL::named_type(References::POI_CHUNK.type_name(), &DSL::remainder_type());
        if poi_chunk_type != df.input_schema().get_type(&References::POI_CHUNK) {
            panic!("java.lang.IllegalStateException: Poi type is not what was expected.");
        }
        let process_records = process_records.clone();
        df.fix_type_everywhere(&name, &poi_chunk_type, move |_, input| {
            input.map_second(|rest| Value::Dynamic(cap(rest.into_dynamic(), &*process_records)))
        })
    })
}

fn cap(input: Dynamic, process_records: &dyn Fn(Vec<Dynamic>) -> Vec<Dynamic>) -> Dynamic {
    input.update("Sections", |sections| {
        sections.update_map_values(|(key, section)| {
            let section = section.update("Records", |records| {
                match records.as_stream_opt().result() {
                    Some(stream) => records.create_list(process_records(stream)),
                    None => records,
                }
            });
            (key, section)
        })
    })
}
