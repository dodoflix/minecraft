//! `ReorganizePoi`: moves the numbered section records of a `POI_CHUNK` under `Sections`.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::value::Value;
use mc_core::java_collections::{HASH_MAP_DEFAULT_CAPACITY, hash_map_ordered};
use mc_dfu::Dynamic;

/// `new ReorganizePoi(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("ReorganizePoi", output_schema, changes_type, |df| {
        let poi_chunk_type =
            DSL::named_type(References::POI_CHUNK.type_name(), &DSL::remainder_type());
        if poi_chunk_type != df.input_schema().get_type(&References::POI_CHUNK) {
            panic!("java.lang.IllegalStateException: Poi type is not what was expected.");
        }
        df.fix_type_everywhere("POI reorganization", &poi_chunk_type, |_, input| {
            input.map_second(|rest| Value::Dynamic(cap(rest.into_dynamic())))
        })
    })
}

/// `ReorganizePoi.cap(input)`.
fn cap(mut input: Dynamic) -> Dynamic {
    // a `HashMap<Dynamic, Dynamic>`: entries in `put` order, reordered below
    let mut sections: Vec<(Dynamic, Dynamic)> = Vec::new();
    for i in 0..16 {
        let key = i.to_string();
        if let Some(section_records) = input.get(&key).result() {
            let new_section = input.create_map([(input.create_string("Records"), section_records)]);
            sections.push((input.create_string(i.to_string()), new_section));
            input = input.remove(&key);
        }
    }
    let sections = hash_map_ordered(sections, |(k, _)| k.hash_code(), HASH_MAP_DEFAULT_CAPACITY);
    input.set("Sections", input.create_map(sections))
}
