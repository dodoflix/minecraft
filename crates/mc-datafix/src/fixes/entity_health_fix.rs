//! `EntityHealthFix`: stores `HealF` / `Health` as a float `Health` (Java's `ENTITIES` set is unused).

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new EntityHealthFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("EntityHealthFix", output_schema, changes_type, |df| {
        df.fix_type_everywhere_typed(
            "EntityHealthFix",
            &df.input_schema().get_type(&References::ENTITY),
            |input| input.update_remainder(fix_tag),
        )
    })
}

fn fix_tag(mut input: Dynamic) -> Dynamic {
    let old_heal_f = input.get("HealF").as_number().result();
    let old_health = input.get("Health").as_number().result();
    let health = if let Some(heal_f) = old_heal_f {
        input = input.remove("HealF");
        heal_f.float_value()
    } else if let Some(health) = old_health {
        health.float_value()
    } else {
        return input;
    };
    input.set("Health", input.create_float(health))
}
