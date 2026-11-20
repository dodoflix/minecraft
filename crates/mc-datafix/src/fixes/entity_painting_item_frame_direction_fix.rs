//! `EntityPaintingItemFrameDirectionFix`: converts the `Direction` / `Dir` of paintings and item frames to `Facing`,
//! shifting the tile position.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use mc_dfu::Dynamic;

const DIRECTIONS: [[i32; 3]; 4] = [[0, 0, 1], [-1, 0, 0], [0, 0, -1], [1, 0, 0]];

/// `new EntityPaintingItemFrameDirectionFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new(
        "EntityPaintingItemFrameDirectionFix",
        output_schema,
        changes_type,
        |df| {
            let input_schema = df.input_schema();
            let painting_type = input_schema.get_choice_type(&References::ENTITY, "Painting");
            let painting_f = DSL::named_choice("Painting", &painting_type);
            let item_frame_type = input_schema.get_choice_type(&References::ENTITY, "ItemFrame");
            let item_frame_f = DSL::named_choice("ItemFrame", &item_frame_type);
            let entity_type = input_schema.get_type(&References::ENTITY);
            let painting_rule =
                df.fix_type_everywhere_typed("EntityPaintingFix", &entity_type, move |input| {
                    input.update_typed_as(&painting_f, &painting_type, |entity| {
                        entity.update_remainder(|tag| do_fix(tag, true, false))
                    })
                });
            let item_frame_rule =
                df.fix_type_everywhere_typed("EntityItemFrameFix", &entity_type, move |input| {
                    input.update_typed_as(&item_frame_f, &item_frame_type, |entity| {
                        entity.update_remainder(|tag| do_fix(tag, false, true))
                    })
                });
            TypeRewriteRule::seq(painting_rule, item_frame_rule)
        },
    )
}

fn do_fix(input: Dynamic, is_painting: bool, is_item_frame: bool) -> Dynamic {
    let mut input = input;
    if (is_painting || is_item_frame) && input.get("Facing").as_number().result().is_none() {
        let direction: i32;
        if input.get("Direction").as_number().result().is_some() {
            direction = i32::from(input.get("Direction").as_byte_or(0)) % DIRECTIONS.len() as i32;
            let Some(steps) = usize::try_from(direction)
                .ok()
                .and_then(|i| DIRECTIONS.get(i))
            else {
                panic!(
                    "java.lang.ArrayIndexOutOfBoundsException: Index {direction} out of bounds for length {}",
                    DIRECTIONS.len()
                )
            };
            input = input.set(
                "TileX",
                input.create_int(input.get("TileX").as_int_or(0).wrapping_add(steps[0])),
            );
            input = input.set(
                "TileY",
                input.create_int(input.get("TileY").as_int_or(0).wrapping_add(steps[1])),
            );
            input = input.set(
                "TileZ",
                input.create_int(input.get("TileZ").as_int_or(0).wrapping_add(steps[2])),
            );
            input = input.remove("Direction");
            if is_item_frame && input.get("ItemRotation").as_number().result().is_some() {
                input = input.set(
                    "ItemRotation",
                    input.create_byte(
                        (i32::from(input.get("ItemRotation").as_byte_or(0)) * 2) as i8,
                    ),
                );
            }
        } else {
            direction = i32::from(input.get("Dir").as_byte_or(0)) % DIRECTIONS.len() as i32;
            input = input.remove("Dir");
        }
        input = input.set("Facing", input.create_byte(direction as i8));
    }
    input
}
