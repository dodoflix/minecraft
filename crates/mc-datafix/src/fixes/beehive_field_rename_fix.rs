//! `BeehiveFieldRenameFix`: drops the bees of a beehive and renames the fields of their data.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::extra_data_fix_utils::cast;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new BeehiveFieldRenameFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("BeehiveFieldRenameFix", output_schema, true, |df| {
        let beehive_type = df
            .input_schema()
            .get_choice_type(&References::BLOCK_ENTITY, "minecraft:beehive");
        let beehive_f = DSL::named_choice("minecraft:beehive", &beehive_type);
        let bees_type = beehive_type.find_field_type("Bees");
        let bee_type = bees_type
            .as_list()
            .expect("the bees of a beehive are a list")
            .clone();
        let bees_f = DSL::field_finder("Bees", &bees_type);
        let bee_f = DSL::type_finder(&bee_type);
        let entity_type = df.input_schema().get_type(&References::BLOCK_ENTITY);
        let new_entity_type = df.output_schema().get_type(&References::BLOCK_ENTITY);
        let target = new_entity_type.clone();
        df.fix_type_everywhere_typed_new(
            "BeehiveFieldRenameFix",
            &entity_type,
            &new_entity_type,
            move |input| {
                cast(
                    &target,
                    input.update_typed(&beehive_f, |beehive| {
                        beehive
                            .update_remainder(fix_beehive)
                            .update_typed(&bees_f, |bees| {
                                bees.update_typed(&bee_f, |bee| bee.update_remainder(fix_bee))
                            })
                    }),
                )
            },
        )
    })
}

fn fix_beehive(beehive: Dynamic) -> Dynamic {
    beehive.remove("Bees")
}

fn fix_bee(bee: Dynamic) -> Dynamic {
    let bee = bee.remove("EntityData");
    let bee = bee.rename_field("TicksInHive", "ticks_in_hive");
    bee.rename_field("MinOccupationTicks", "min_ticks_in_hive")
}
