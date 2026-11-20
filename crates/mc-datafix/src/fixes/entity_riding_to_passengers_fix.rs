//! `EntityRidingToPassengersFix`: turns the `Riding` chain of entity trees into the `Passengers` list of the vehicle.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::extra_data_fix_utils::{cast_value, patch_sub_type};
use crate::finder::OpticFinder;
use crate::fixes::references::References;
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use crate::ty::Type;
use crate::typed::Typed;
use crate::value::Value;
use mc_dfu::Ops;

const NO_VALUE: &str = "java.util.NoSuchElementException: No value present";

/// `new EntityRidingToPassengersFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new(
        "EntityRidingToPassengersFix",
        output_schema,
        changes_type,
        |df| {
            let (input_schema, output_schema) = (df.input_schema(), df.output_schema());
            let old_entity_tree_type = input_schema.get_type_raw(&References::ENTITY_TREE);
            let new_entity_tree_type = output_schema.get_type_raw(&References::ENTITY_TREE);
            let entity_type = input_schema.get_type_raw(&References::ENTITY);
            let old_type = DSL::named_type(
                References::ENTITY_TREE.type_name(),
                &DSL::and_type(
                    &DSL::optional_type(&DSL::field_type("Riding", &old_entity_tree_type)),
                    &entity_type,
                ),
            );
            let new_type = DSL::named_type(
                References::ENTITY_TREE.type_name(),
                &DSL::and_type(
                    &DSL::optional_type(&DSL::field_type(
                        "Passengers",
                        &DSL::list_type(&new_entity_tree_type),
                    )),
                    &entity_type,
                ),
            );
            if input_schema.get_type(&References::ENTITY_TREE) != old_type {
                panic!(
                    "java.lang.IllegalStateException: Old entity type is not what was expected."
                );
            }
            if !output_schema
                .get_type(&References::ENTITY_TREE)
                .equals_with(&new_type, true, true)
            {
                panic!(
                    "java.lang.IllegalStateException: New entity type is not what was expected."
                );
            }
            let patched_entity_tree_type = patch_sub_type(&old_type, &old_type, &new_type);
            let entity_finder = DSL::type_finder(&entity_type);
            let new_entity_tree_value_finder = DSL::type_finder(&new_type);
            let riding_finder = DSL::field_finder("Riding", &new_entity_tree_type);
            let old_player_type = input_schema.get_type(&References::PLAYER);
            let new_player_type = output_schema.get_type(&References::PLAYER);
            let riding = df.fix_type_everywhere_new(
                "EntityRidingToPassengerFix",
                &old_type,
                &new_type,
                move |ops, badly_typed_input| {
                    let input = cast_value(&patched_entity_tree_type, badly_typed_input, *ops);
                    let maybe_riding = input
                        .get_optional_typed(&riding_finder)
                        .and_then(|t| t.get_optional(&new_entity_tree_value_finder));
                    let entity = input
                        .get_optional(&entity_finder)
                        .unwrap_or_else(|| panic!("{NO_VALUE}"));
                    match maybe_riding {
                        None => Value::named(
                            References::ENTITY_TREE.type_name(),
                            Value::pair(Value::none(), entity),
                        ),
                        Some(riding) => add_passenger_to_top(
                            riding,
                            entity,
                            ops,
                            &new_entity_tree_type,
                            &new_entity_tree_value_finder,
                        ),
                    }
                },
            );
            TypeRewriteRule::seq(
                riding,
                df.write_and_read(
                    "player RootVehicle injecter",
                    &old_player_type,
                    &new_player_type,
                ),
            )
        },
    )
}

fn add_passenger_to_top(
    root: Value,
    passenger_entity: Value,
    ops: &Ops,
    raw_entity_tree_type: &Type,
    entity_tree_finder: &OpticFinder,
) -> Value {
    let (_, (passengers, root_entity)) = {
        let (name, rest) = root.into_pair();
        (name, rest.into_pair())
    };
    let new_passenger = match passengers.into_option().map(Value::into_list) {
        Some(mut passengers) if !passengers.is_empty() => {
            let unwrapped_passenger = unwrap_recursive_value(
                passengers.swap_remove(0),
                ops,
                raw_entity_tree_type,
                entity_tree_finder,
            );
            add_passenger_to_top(
                unwrapped_passenger,
                passenger_entity,
                ops,
                raw_entity_tree_type,
                entity_tree_finder,
            )
        }
        _ => Value::named(
            References::ENTITY_TREE.type_name(),
            Value::pair(Value::none(), passenger_entity),
        ),
    };
    let new_passengers = vec![wrap_recursive_value(
        new_passenger,
        ops,
        raw_entity_tree_type,
        entity_tree_finder,
    )];
    Value::named(
        References::ENTITY_TREE.type_name(),
        Value::pair(Value::some(Value::List(new_passengers)), root_entity),
    )
}

fn unwrap_recursive_value(
    raw: Value,
    ops: &Ops,
    raw_type: &Type,
    value_finder: &OpticFinder,
) -> Value {
    Typed::new(raw_type.clone(), *ops, raw)
        .get_optional(value_finder)
        .unwrap_or_else(|| panic!("{NO_VALUE}"))
}

fn wrap_recursive_value(
    value: Value,
    ops: &Ops,
    raw_type: &Type,
    value_finder: &OpticFinder,
) -> Value {
    raw_type
        .point_typed(ops)
        .unwrap_or_else(|| panic!("{NO_VALUE}"))
        .set(value_finder, value)
        .into_value()
}
