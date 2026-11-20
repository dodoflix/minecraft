//! `FoodToConsumableFix`: moves the eating time and effects of `minecraft:food` into `minecraft:consumable` and
//! `using_converts_to` into `minecraft:use_remainder`.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new FoodToConsumableFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("FoodToConsumableFix", output_schema, true, |df| {
        df.write_fix_and_read(
            "Food to consumable fix",
            &df.input_schema().get_type(&References::DATA_COMPONENTS),
            &df.output_schema().get_type(&References::DATA_COMPONENTS),
            |components| {
                let Some(food_component) = components.get("minecraft:food").result() else {
                    return components;
                };
                let eat_seconds = food_component.get("eat_seconds").as_float_or(1.6);
                let on_consume_effects: Vec<Dynamic> = food_component
                    .get("effects")
                    .as_stream()
                    .into_iter()
                    .map(|effect| {
                        effect
                            .empty_map()
                            .set("type", effect.create_string("minecraft:apply_effects"))
                            .set("effects", effect.create_list(effect.get("effect").result()))
                            .set(
                                "probability",
                                effect.create_float(effect.get("probability").as_float_or(1.0)),
                            )
                    })
                    .collect();
                let components = Dynamic::copy_field(
                    &food_component,
                    "using_converts_to",
                    &components,
                    "minecraft:use_remainder",
                );
                let components = components.set(
                    "minecraft:food",
                    food_component
                        .remove("eat_seconds")
                        .remove("effects")
                        .remove("using_converts_to"),
                );
                components.set(
                    "minecraft:consumable",
                    components
                        .empty_map()
                        .set("consume_seconds", components.create_float(eat_seconds))
                        .set(
                            "on_consume_effects",
                            components.create_list(on_consume_effects),
                        ),
                )
            },
        )
    })
}
