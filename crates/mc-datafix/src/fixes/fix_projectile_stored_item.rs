//! `FixProjectileStoredItem`: gives arrows and tridents the item stack they were shot from.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::extra_data_fix_utils::{cast, chain_all_filters};
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::Type;
use crate::typed::Typed;
use crate::util::write_and_read_typed_or_throw;
use mc_dfu::Dynamic;

const EMPTY_POTION: &str = "minecraft:empty";

type SubFixer = fn(Typed, &Type) -> Typed;
type Filter = Box<dyn Fn(Typed) -> Typed + Send + Sync>;

/// `new FixProjectileStoredItem(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("FixProjectileStoredItem", output_schema, true, |df| {
        let input_entity_type = df.input_schema().get_type(&References::ENTITY);
        let output_entity_type = df.output_schema().get_type(&References::ENTITY);
        df.fix_type_everywhere_typed_new(
            "Fix AbstractArrow item type",
            &input_entity_type,
            &output_entity_type,
            chain_all_filters(vec![
                fix_choice(df, "minecraft:trident", cast_unchecked),
                fix_choice(df, "minecraft:arrow", fix_arrow),
                fix_choice(df, "minecraft:spectral_arrow", fix_spectral_arrow),
            ]),
        )
    })
}

fn fix_choice(df: &DataFix, entity_name: &str, fixer: SubFixer) -> Filter {
    let input_entity_choice_type = df
        .input_schema()
        .get_choice_type(&References::ENTITY, entity_name);
    let output_entity_choice_type = df
        .output_schema()
        .get_choice_type(&References::ENTITY, entity_name);
    let entity_f = DSL::named_choice(entity_name, &input_entity_choice_type);
    Box::new(move |input| {
        input.update_typed_as(&entity_f, &output_entity_choice_type, |typed| {
            fixer(typed, &output_entity_choice_type)
        })
    })
}

fn fix_arrow(typed: Typed, output_type: &Type) -> Typed {
    write_and_read_typed_or_throw(&typed, output_type, |input| {
        let item = create_item_stack(&input, get_arrow_type(&input));
        input.set("item", item)
    })
}

fn get_arrow_type(input: &Dynamic) -> &'static str {
    if input.get("Potion").as_string_or(EMPTY_POTION) == EMPTY_POTION {
        "minecraft:arrow"
    } else {
        "minecraft:tipped_arrow"
    }
}

fn fix_spectral_arrow(typed: Typed, output_type: &Type) -> Typed {
    write_and_read_typed_or_throw(&typed, output_type, |input| {
        let item = create_item_stack(&input, "minecraft:spectral_arrow");
        input.set("item", item)
    })
}

fn create_item_stack(input: &Dynamic, item_name: &str) -> Dynamic {
    input.create_map([
        (input.create_string("id"), input.create_string(item_name)),
        (input.create_string("Count"), input.create_int(1)),
    ])
}

fn cast_unchecked(input: Typed, output_type: &Type) -> Typed {
    cast(output_type, input)
}
