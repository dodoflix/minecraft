//! `EquipmentFormatFix`: converts `ArmorItems`, `HandItems`, `body_armor_item` and `saddle` of `entity_equipment` into
//! the `equipment` map keyed by slot, dropping placeholder (id-less) stacks.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::typed::Typed;
use crate::value::Value;
use mc_dfu::Dynamic;

/// `new EquipmentFormatFix(outputSchema)` (`changesType` is `true`).
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("EquipmentFormatFix", output_schema, true, |df| {
        let (input_schema, output_schema) = (df.input_schema(), df.output_schema());
        let old_item_stack_type = input_schema.get_type_raw(&References::ITEM_STACK);
        let new_item_stack_type = output_schema.get_type_raw(&References::ITEM_STACK);
        let id_finder = old_item_stack_type.find_field("id");
        let old_equipment_type = DSL::named_type(
            References::ENTITY_EQUIPMENT.type_name(),
            &DSL::and_type4(
                &DSL::optional_type(&DSL::field_type(
                    "ArmorItems",
                    &DSL::list_type(&old_item_stack_type),
                )),
                &DSL::optional_type(&DSL::field_type(
                    "HandItems",
                    &DSL::list_type(&old_item_stack_type),
                )),
                &DSL::optional_type(&DSL::field_type("body_armor_item", &old_item_stack_type)),
                &DSL::optional_type(&DSL::field_type("saddle", &old_item_stack_type)),
            ),
        );
        let optional_stack =
            |name: &str| DSL::optional_type(&DSL::field_type(name, &new_item_stack_type));
        let new_equipment_type = DSL::named_type(
            References::ENTITY_EQUIPMENT.type_name(),
            &DSL::optional_type(&DSL::field_type(
                "equipment",
                &DSL::and_type4(
                    &optional_stack("mainhand"),
                    &optional_stack("offhand"),
                    &optional_stack("feet"),
                    &DSL::and_type4(
                        &optional_stack("legs"),
                        &optional_stack("chest"),
                        &optional_stack("head"),
                        &DSL::and_type3(
                            &optional_stack("body"),
                            &optional_stack("saddle"),
                            &DSL::remainder_type(),
                        ),
                    ),
                ),
            )),
        );
        if old_equipment_type != input_schema.get_type(&References::ENTITY_EQUIPMENT) {
            panic!(
                "java.lang.IllegalStateException: Input entity_equipment type does not match expected"
            );
        }
        if new_equipment_type != output_schema.get_type(&References::ENTITY_EQUIPMENT) {
            panic!(
                "java.lang.IllegalStateException: Output entity_equipment type does not match expected"
            );
        }
        df.fix_type_everywhere_new(
            "EquipmentFormatFix",
            &old_equipment_type,
            &new_equipment_type,
            move |ops, named_old_equipment| {
                let is_placeholder = |item_stack: &Value| {
                    let typed = Typed::new(old_item_stack_type.clone(), *ops, item_stack.clone());
                    typed.get_optional(&id_finder).is_none()
                };
                let (type_name, old_equipment) = named_old_equipment.into_named();
                let (armor_items, rest) = old_equipment.into_pair();
                let (hand_items, rest) = rest.into_pair();
                let (body, saddle) = rest.into_pair();
                let armor_items = armor_items
                    .into_option()
                    .map_or_else(Vec::new, Value::into_list);
                let hand_items = hand_items
                    .into_option()
                    .map_or_else(Vec::new, Value::into_list);
                let feet = get_item_from_list(0, &armor_items, &is_placeholder);
                let legs = get_item_from_list(1, &armor_items, &is_placeholder);
                let chest = get_item_from_list(2, &armor_items, &is_placeholder);
                let head = get_item_from_list(3, &armor_items, &is_placeholder);
                let mainhand = get_item_from_list(0, &hand_items, &is_placeholder);
                let offhand = get_item_from_list(1, &hand_items, &is_placeholder);
                if are_all_empty(&[
                    &body, &saddle, &feet, &legs, &chest, &head, &mainhand, &offhand,
                ]) {
                    Value::named(type_name, Value::none())
                } else {
                    Value::named(
                        type_name,
                        Value::some(Value::pair(
                            mainhand,
                            Value::pair(
                                offhand,
                                Value::pair(
                                    feet,
                                    Value::pair(
                                        legs,
                                        Value::pair(
                                            chest,
                                            Value::pair(
                                                head,
                                                Value::pair(
                                                    body,
                                                    Value::pair(
                                                        saddle,
                                                        Value::Dynamic(Dynamic::empty(*ops)),
                                                    ),
                                                ),
                                            ),
                                        ),
                                    ),
                                ),
                            ),
                        )),
                    )
                }
            },
        )
    })
}

fn are_all_empty(fields: &[&Value]) -> bool {
    fields.iter().all(|field| !field.is_left())
}

fn get_item_from_list(
    index: usize,
    items: &[Value],
    is_placeholder: &impl Fn(&Value) -> bool,
) -> Value {
    match items.get(index) {
        None => Value::none(),
        Some(item) if is_placeholder(item) => Value::none(),
        Some(item) => Value::some(item.clone()),
    }
}
