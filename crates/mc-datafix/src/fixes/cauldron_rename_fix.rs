//! `CauldronRenameFix`: `minecraft:cauldron` with a non-zero `level` becomes `minecraft:water_cauldron`; an empty one
//! loses its `Properties`.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new CauldronRenameFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("CauldronRenameFix", output_schema, changes_type, |df| {
        df.fix_type_everywhere_typed(
            "cauldron_rename_fix",
            &df.input_schema().get_type(&References::BLOCK_STATE),
            |input| input.update_remainder(fix),
        )
    })
}

fn fix(tag: Dynamic) -> Dynamic {
    if tag.get("Name").as_string_lossy().result().as_deref() != Some("minecraft:cauldron") {
        return tag;
    }
    let properties = tag.get("Properties").or_else_empty_map();
    if properties.get("level").as_string_or("0") == "0" {
        tag.remove("Properties")
    } else {
        tag.set("Name", tag.create_string("minecraft:water_cauldron"))
    }
}
