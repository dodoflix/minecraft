//! `ChestedHorsesInventoryZeroIndexingFix`: shifts the inventory slots of chested horses to start at zero.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::finder::OpticFinder;
use crate::fixes::references::References;
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use crate::ty::Type;
use crate::value::Value;

/// `new ChestedHorsesInventoryZeroIndexingFix(v3807)`.
pub fn new(v3807: &Schema) -> DataFix {
    DataFix::new(
        "ChestedHorsesInventoryZeroIndexingFix",
        v3807,
        false,
        |df| {
            let item_stack_finder =
                DSL::type_finder(&df.input_schema().get_type(&References::ITEM_STACK));
            let entity_type = df.input_schema().get_type(&References::ENTITY);
            let fixer = |horse_id| {
                horse_like_inventory_indexing_fixer(df, &item_stack_finder, &entity_type, horse_id)
            };
            TypeRewriteRule::seq_all(
                fixer("minecraft:llama"),
                vec![
                    fixer("minecraft:trader_llama"),
                    fixer("minecraft:mule"),
                    fixer("minecraft:donkey"),
                ],
            )
        },
    )
}

fn horse_like_inventory_indexing_fixer(
    df: &DataFix,
    item_stack_finder: &OpticFinder,
    schema: &Type,
    horse_id: &str,
) -> TypeRewriteRule {
    let choice_type = df
        .input_schema()
        .get_choice_type(&References::ENTITY, horse_id);
    let entity_finder = DSL::named_choice(horse_id, &choice_type);
    let items_field_finder = choice_type.find_field("Items");
    let item_stack_finder = item_stack_finder.clone();
    df.fix_type_everywhere_typed(
        &format!("Fix non-zero indexing in chest horse type {horse_id}"),
        schema,
        move |input| {
            input.update_typed(&entity_finder, |horse_like| {
                horse_like.update_typed(&items_field_finder, |items| {
                    items.update(&item_stack_finder, |named_stack| {
                        named_stack.map_second(|item_stack| {
                            item_stack.map_second(|pair| {
                                pair.map_second(|remainder| {
                                    Value::Dynamic(remainder.into_dynamic().update(
                                        "Slot",
                                        |slot| {
                                            slot.create_byte(slot.as_int_or(2).wrapping_sub(2) as i8)
                                        },
                                    ))
                                })
                            })
                        })
                    })
                })
            })
        },
    )
}
