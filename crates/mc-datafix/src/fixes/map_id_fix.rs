//! `MapIdFix`: wraps the map id counter tag into a `data` compound.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new MapIdFix(schema)`.
pub fn new(schema: &Schema) -> DataFix {
    DataFix::new("MapIdFix", schema, false, |df| {
        df.fix_type_everywhere_typed(
            "Map id fix",
            &df.input_schema()
                .get_type(&References::SAVED_DATA_MAP_INDEX),
            |input| {
                input.update_remainder(|tag| {
                    tag.create_map([(tag.create_string("data"), tag.clone())])
                })
            },
        )
    })
}
