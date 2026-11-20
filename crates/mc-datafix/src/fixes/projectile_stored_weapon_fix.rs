//! `ProjectileStoredWeaponFix`: re-reads arrows and spectral arrows with the type of the output schema.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::extra_data_fix_utils::chain_all_filters;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::typed::Typed;
use crate::util::write_and_read_typed_or_throw;

type Filter = Box<dyn Fn(Typed) -> Typed + Send + Sync>;

/// `new ProjectileStoredWeaponFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("ProjectileStoredWeaponFix", output_schema, true, |df| {
        let input_entity_type = df.input_schema().get_type(&References::ENTITY);
        let output_entity_type = df.output_schema().get_type(&References::ENTITY);
        df.fix_type_everywhere_typed_new(
            "Fix Arrow stored weapon",
            &input_entity_type,
            &output_entity_type,
            chain_all_filters(vec![
                fix_choice(df, "minecraft:arrow"),
                fix_choice(df, "minecraft:spectral_arrow"),
            ]),
        )
    })
}

fn fix_choice(df: &DataFix, entity_name: &str) -> Filter {
    let input_entity_choice_type = df
        .input_schema()
        .get_choice_type(&References::ENTITY, entity_name);
    let output_entity_choice_type = df
        .output_schema()
        .get_choice_type(&References::ENTITY, entity_name);
    let entity_f = DSL::named_choice(entity_name, &input_entity_choice_type);
    Box::new(move |input| {
        input.update_typed_as(&entity_f, &output_entity_choice_type, |typed| {
            write_and_read_typed_or_throw(&typed, &output_entity_choice_type, |d| d)
        })
    })
}
