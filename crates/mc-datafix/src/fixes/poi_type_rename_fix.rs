//! `PoiTypeRenameFix`: renames the `type` of every record of a `POI_CHUNK` section.

use crate::datafix::DataFix;
use crate::fixes::abstract_poi_section_fix;
use crate::schema::Schema;

/// `new PoiTypeRenameFix(outputSchema, name, renamer)`.
pub fn new(
    schema: &Schema,
    name: &str,
    renamer: impl Fn(&str) -> String + Send + Sync + 'static,
) -> DataFix {
    abstract_poi_section_fix::new("PoiTypeRenameFix", schema, name, move |stream| {
        stream
            .into_iter()
            .map(|element| {
                element.update("type", |ty| match ty.as_string().result() {
                    Some(s) => ty.create_string(s.map_lossless(|t| renamer(t))),
                    None => ty,
                })
            })
            .collect()
    })
}
