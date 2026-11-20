//! `FurnaceRecipeFix`: moves the `RecipeLocation<i>` / `RecipeAmount<i>` pairs of furnaces, blast furnaces and smokers
//! into a typed `RecipesUsed` compound list.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::finder::remainder_finder;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::Type;
use crate::typed::Typed;
use crate::value::Value;

/// `new FurnaceRecipeFix(schema, changesType)`.
pub fn new(schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("FurnaceRecipeFix", schema, changes_type, |df| {
        let (input_schema, output_schema) = (df.input_schema(), df.output_schema());
        let recipe_type = output_schema.get_type_raw(&References::RECIPE);
        let replaced_type = DSL::and_type(
            &DSL::optional_type(&DSL::field_type(
                "RecipesUsed",
                &DSL::and_type(
                    &DSL::compound_list_type_kv(&recipe_type, &DSL::int_type()),
                    &DSL::remainder_type(),
                ),
            )),
            &DSL::remainder_type(),
        );
        let choice =
            |schema: &Schema, name: &str| schema.get_choice_type(&References::BLOCK_ENTITY, name);
        let old_furnace_f = DSL::named_choice(
            "minecraft:furnace",
            &choice(&input_schema, "minecraft:furnace"),
        );
        let old_blast_furnace_f = DSL::named_choice(
            "minecraft:blast_furnace",
            &choice(&input_schema, "minecraft:blast_furnace"),
        );
        let old_smoker_f = DSL::named_choice(
            "minecraft:smoker",
            &choice(&input_schema, "minecraft:smoker"),
        );
        let new_furnace_type = choice(&output_schema, "minecraft:furnace");
        let new_blast_furnace_type = choice(&output_schema, "minecraft:blast_furnace");
        let new_smoker_type = choice(&output_schema, "minecraft:smoker");
        let old_entity_type = input_schema.get_type(&References::BLOCK_ENTITY);
        let new_entity_type = output_schema.get_type(&References::BLOCK_ENTITY);
        df.fix_type_everywhere_typed_new(
            "FurnaceRecipesFix",
            &old_entity_type,
            &new_entity_type,
            move |input| {
                input
                    .update_typed_as(&old_furnace_f, &new_furnace_type, |furnace| {
                        update_furnace_contents(&recipe_type, &replaced_type, furnace)
                    })
                    .update_typed_as(&old_blast_furnace_f, &new_blast_furnace_type, |blast| {
                        update_furnace_contents(&recipe_type, &replaced_type, blast)
                    })
                    .update_typed_as(&old_smoker_f, &new_smoker_type, |smoker| {
                        update_furnace_contents(&recipe_type, &replaced_type, smoker)
                    })
            },
        )
    })
}

fn update_furnace_contents(recipe_type: &Type, replaced_type: &Type, input: Typed) -> Typed {
    let mut tag = input.get_remainder();
    let recipes_used_size = tag.get("RecipesUsedSize").as_int_or(0);
    tag = tag.remove("RecipesUsedSize");
    let mut results = Vec::new();

    for i in 0..recipes_used_size {
        let location_key = format!("RecipeLocation{i}");
        let amount_key = format!("RecipeAmount{i}");
        let maybe_location = tag.get(&location_key).result();
        let amount = tag.get(&amount_key).as_int_or(0);
        if amount > 0 {
            let parsed = maybe_location.and_then(|location| recipe_type.read(&location).result());
            if let Some((recipe, _)) = parsed {
                results.push(Value::pair(recipe, Value::Int(amount)));
            }
        }
        tag = tag.remove(&location_key).remove(&amount_key);
    }

    let empty_map = tag.empty_map();
    input.set_with_type(
        &remainder_finder(),
        replaced_type,
        Value::pair(
            Value::some(Value::pair(Value::List(results), Value::Dynamic(empty_map))),
            Value::Dynamic(tag),
        ),
    )
}
