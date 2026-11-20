//! `PoiTypeRemoveFix`: removes the POI records whose type matches a predicate.

use crate::datafix::DataFix;
use crate::fixes::abstract_poi_section_fix;
use crate::schema::Schema;

/// `new PoiTypeRemoveFix(outputSchema, name, typesToRemove)`.
pub fn new(
    output_schema: &Schema,
    name: &str,
    types_to_remove: impl Fn(&str) -> bool + Send + Sync + 'static,
) -> DataFix {
    abstract_poi_section_fix::new("PoiTypeRemoveFix", output_schema, name, move |records| {
        records
            .into_iter()
            .filter(|record| {
                record
                    .get("type")
                    .as_string()
                    .result()
                    .is_some_and(|ty| !types_to_remove(ty.as_lossy_str()))
            })
            .collect()
    })
}
