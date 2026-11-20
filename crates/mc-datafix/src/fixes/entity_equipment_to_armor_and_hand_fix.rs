//! `EntityEquipmentToArmorAndHandFix`: converts the `Equipment` list of mobs to `ArmorItems` / `HandItems` and the drop
//! chances to `ArmorDropChances` / `HandDropChances`.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use crate::value::Value;
use mc_dfu::Dynamic;

/// `new EntityEquipmentToArmorAndHandFix(outputSchema)` (`changesType` is `true`).
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "EntityEquipmentToArmorAndHandFix",
        output_schema,
        true,
        |df| {
            let (input_schema, output_schema) = (df.input_schema(), df.output_schema());
            let old_item_stack_type = input_schema.get_type_raw(&References::ITEM_STACK);
            let new_item_stack_type = output_schema.get_type_raw(&References::ITEM_STACK);
            let old_equipment_type = DSL::named_type(
                References::ENTITY_EQUIPMENT.type_name(),
                &DSL::optional_type(&DSL::field_type(
                    "Equipment",
                    &DSL::list_type(&old_item_stack_type),
                )),
            );
            let new_equipment_type = DSL::named_type(
                References::ENTITY_EQUIPMENT.type_name(),
                &DSL::and_type4(
                    &DSL::optional_type(&DSL::field_type(
                        "ArmorItems",
                        &DSL::list_type(&new_item_stack_type),
                    )),
                    &DSL::optional_type(&DSL::field_type(
                        "HandItems",
                        &DSL::list_type(&new_item_stack_type),
                    )),
                    &DSL::optional_type(&DSL::field_type("body_armor_item", &new_item_stack_type)),
                    &DSL::optional_type(&DSL::field_type("saddle", &new_item_stack_type)),
                ),
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
            let drop_chances = df.fix_type_everywhere_typed(
                "EntityEquipmentToArmorAndHandFix - drop chances",
                &input_schema.get_type(&References::ENTITY),
                |typed| typed.update_remainder(fix_drop_chances),
            );
            let equipment = df.fix_type_everywhere_new(
                "EntityEquipmentToArmorAndHandFix - equipment",
                &old_equipment_type,
                &new_equipment_type,
                move |ops, named| {
                    let empty_stack = new_item_stack_type
                        .read(&Dynamic::empty(*ops).empty_map())
                        .result()
                        .unwrap_or_else(|| {
                            panic!(
                                "java.lang.IllegalStateException: Could not parse newly created empty itemstack."
                            )
                        })
                        .0;
                    named.map_second(|equipment_field| {
                        let mut items = equipment_field
                            .into_option()
                            .map_or_else(Vec::new, Value::into_list);
                        let mut hand_items = Value::none();
                        let mut armor_items = Value::none();
                        if !items.is_empty() {
                            hand_items =
                                Value::some(Value::List(vec![items[0].clone(), empty_stack.clone()]));
                        }
                        if items.len() > 1 {
                            let mut armor = vec![empty_stack.clone(); 4];
                            for i in 1..items.len().min(5) {
                                armor[i - 1] = std::mem::replace(&mut items[i], Value::Unit);
                            }
                            armor_items = Value::some(Value::List(armor));
                        }
                        Value::pair(
                            armor_items,
                            Value::pair(hand_items, Value::pair(Value::none(), Value::none())),
                        )
                    })
                },
            );
            TypeRewriteRule::seq(drop_chances, equipment)
        },
    )
}

fn fix_drop_chances(tag: Dynamic) -> Dynamic {
    let drop_chances = tag.get("DropChances").as_stream_opt().result();
    let mut tag = tag.remove("DropChances");
    if let Some(values) = drop_chances {
        // `Stream.concat(values.map(asFloat(0)), Stream.generate(() -> 0))`
        let mut chances = values
            .into_iter()
            .map(|v| v.as_float_or(0.0))
            .chain(std::iter::repeat(0.0f32));
        let hand_chance = chances.next().unwrap();
        if !tag.get("HandDropChances").is_present() {
            tag = tag.set(
                "HandDropChances",
                tag.create_list([tag.create_float(hand_chance), tag.create_float(0.0)]),
            );
        }
        if !tag.get("ArmorDropChances").is_present() {
            let armor: Vec<Dynamic> = (0..4)
                .map(|_| tag.create_float(chances.next().unwrap()))
                .collect();
            tag = tag.set("ArmorDropChances", tag.create_list(armor));
        }
    }
    tag
}
