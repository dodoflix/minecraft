//! `net.minecraft.util.datafix.fixes.AbstractArrowPickupFix`.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::typed::Typed;
use mc_dfu::Dynamic;

/// `new AbstractArrowPickupFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("AbstractArrowPickupFix", output_schema, false, |df| {
        let (input_schema, output_schema) = (df.input_schema(), df.output_schema());
        df.fix_type_everywhere_typed(
            "AbstractArrowPickupFix",
            &input_schema.get_type(&References::ENTITY),
            move |input| update_projectiles(input, &input_schema, &output_schema),
        )
    })
}

fn update_projectiles(input: Typed, input_schema: &Schema, output_schema: &Schema) -> Typed {
    let input = update_entity(input, input_schema, output_schema, "minecraft:arrow");
    let input = update_entity(
        input,
        input_schema,
        output_schema,
        "minecraft:spectral_arrow",
    );
    update_entity(input, input_schema, output_schema, "minecraft:trident")
}

fn update_pickup(tag: Dynamic) -> Dynamic {
    if tag.get("pickup").is_present() {
        tag
    } else {
        let from_player = tag.get("player").as_boolean_or(true);
        tag.set("pickup", tag.create_byte(i8::from(from_player)))
            .remove("player")
    }
}

fn update_entity(input: Typed, input_schema: &Schema, output_schema: &Schema, name: &str) -> Typed {
    let old_type = input_schema.get_choice_type(&References::ENTITY, name);
    let new_type = output_schema.get_choice_type(&References::ENTITY, name);
    input.update_typed_as(&DSL::named_choice(name, &old_type), &new_type, |entity| {
        entity.update_remainder(update_pickup)
    })
}
