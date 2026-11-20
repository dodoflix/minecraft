//! `VillagerTradeFix`: renames carved pumpkins in villager trades to pumpkins.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::finder::OpticFinder;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::typed::Typed;
use crate::value::Value;

/// `new VillagerTradeFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("VillagerTradeFix", output_schema, false, |df| {
        let recipe_type = df.input_schema().get_type(&References::VILLAGER_TRADE);
        let buy_finder = recipe_type.find_field("buy");
        let buy_b_finder = recipe_type.find_field("buyB");
        let sell_finder = recipe_type.find_field("sell");
        let id_f = DSL::field_finder(
            "id",
            &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()),
        );
        df.fix_type_everywhere_typed("Villager trade fix", &recipe_type, move |recipe| {
            recipe
                .update_typed(&buy_finder, |stack| update_item_stack(&id_f, stack))
                .update_typed(&buy_b_finder, |stack| update_item_stack(&id_f, stack))
                .update_typed(&sell_finder, |stack| update_item_stack(&id_f, stack))
        })
    })
}

fn update_item_stack(id_f: &OpticFinder, item_stack: Typed) -> Typed {
    item_stack.update(id_f, |pair| {
        pair.map_second(|name| {
            if name.as_str() == "minecraft:carved_pumpkin" {
                Value::str("minecraft:pumpkin")
            } else {
                name
            }
        })
    })
}
