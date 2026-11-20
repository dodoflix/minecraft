//! `StructureReferenceCountFix`: raises a structure feature's `references` to at least 1.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new StructureReferenceCountFix(outputSchema, changesType)`.
pub fn new(schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("StructureReferenceCountFix", schema, changes_type, |df| {
        let structure_info = df.input_schema().get_type(&References::STRUCTURE_FEATURE);
        df.fix_type_everywhere_typed("Structure Reference Fix", &structure_info, |input| {
            input.update_remainder(set_count_to_at_least_one)
        })
    })
}

fn set_count_to_at_least_one(structure_tag: Dynamic) -> Dynamic {
    structure_tag.update("references", |references| {
        let count = references
            .as_number()
            .result()
            .map(|n| n.int_value())
            .filter(|n| *n > 0)
            .unwrap_or(1);
        references.create_int(count)
    })
}
