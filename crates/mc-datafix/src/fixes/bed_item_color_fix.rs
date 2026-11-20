//! `BedItemColorFix`: gives `minecraft:bed` items with damage 0 the damage 14 (red).

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::finder::remainder_finder;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new BedItemColorFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("BedItemColorFix", output_schema, changes_type, |df| {
        let id_f = DSL::field_finder(
            "id",
            &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()),
        );
        df.fix_type_everywhere_typed(
            "BedItemColorFix",
            &df.input_schema().get_type(&References::ITEM_STACK),
            move |input| {
                let is_bed = input
                    .get_optional(&id_f)
                    .is_some_and(|id| id.second().as_lossy_str() == "minecraft:bed");
                if is_bed {
                    let tag = input.get(&remainder_finder()).into_dynamic();
                    if tag.get("Damage").as_int_or(0) == 0 {
                        return input.set_remainder(tag.set("Damage", tag.create_short(14)));
                    }
                }
                input
            },
        )
    })
}
