//! `CatTypeFix`: changes the cat type 9 to 10.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new CatTypeFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    named_entity_fix::new(
        "CatTypeFix",
        output_schema,
        changes_type,
        "CatTypeFix",
        &References::ENTITY,
        "minecraft:cat",
        |entity| entity.update_remainder(fix_tag),
    )
}

/// `CatTypeFix.fixTag(input)`.
pub fn fix_tag(input: Dynamic) -> Dynamic {
    if input.get("CatType").as_int_or(0) == 9 {
        input.set("CatType", input.create_int(10))
    } else {
        input
    }
}
