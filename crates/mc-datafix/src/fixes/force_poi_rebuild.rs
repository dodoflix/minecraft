//! `ForcePoiRebuild`: removes the `Valid` flag of every section of a `POI_CHUNK`, forcing a rebuild.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::value::Value;
use mc_dfu::Dynamic;

/// `new ForcePoiRebuild(outputSchema, changesType)`.
pub fn new(schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("ForcePoiRebuild", schema, changes_type, |df| {
        let poi_chunk_type =
            DSL::named_type(References::POI_CHUNK.type_name(), &DSL::remainder_type());
        if poi_chunk_type != df.input_schema().get_type(&References::POI_CHUNK) {
            panic!("java.lang.IllegalStateException: Poi type is not what was expected.");
        }
        df.fix_type_everywhere("POI rebuild", &poi_chunk_type, |_, input| {
            input.map_second(|rest| Value::Dynamic(cap(rest.into_dynamic())))
        })
    })
}

fn cap(input: Dynamic) -> Dynamic {
    input.update("Sections", |sections| {
        sections.update_map_values(|(key, section)| (key, section.remove("Valid")))
    })
}
