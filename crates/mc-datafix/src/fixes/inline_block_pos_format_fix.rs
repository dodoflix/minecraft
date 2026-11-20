//! `InlineBlockPosFormatFix`: block positions stored as separate coordinate fields in players and entities become int
//! lists with snake_case names.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::extra_data_fix_utils::{create_block_pos, fix_inline_block_pos};
use crate::finder::OpticFinder;
use crate::fixes::references::References;
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new InlineBlockPosFormatFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("InlineBlockPosFormatFix", output_schema, false, |df| {
        let entity_finder = |choice_name: &str| -> OpticFinder {
            DSL::named_choice(
                choice_name,
                &df.input_schema()
                    .get_choice_type(&References::ENTITY, choice_name),
            )
        };
        let vex_finder = entity_finder("minecraft:vex");
        let phantom_finder = entity_finder("minecraft:phantom");
        let turtle_finder = entity_finder("minecraft:turtle");
        let block_attached_finders = vec![
            entity_finder("minecraft:item_frame"),
            entity_finder("minecraft:glow_item_frame"),
            entity_finder("minecraft:painting"),
            entity_finder("minecraft:leash_knot"),
        ];
        TypeRewriteRule::seq_list(vec![
            df.fix_type_everywhere_typed(
                "InlineBlockPosFormatFix - player",
                &df.input_schema().get_type(&References::PLAYER),
                |player| player.update_remainder(fix_player),
            ),
            df.fix_type_everywhere_typed(
                "InlineBlockPosFormatFix - entity",
                &df.input_schema().get_type(&References::ENTITY),
                move |entity| {
                    let mut entity = entity
                        .update_remainder(fix_living_entity)
                        .update_typed(&vex_finder, |vex| vex.update_remainder(fix_vex))
                        .update_typed(&phantom_finder, |phantom| {
                            phantom.update_remainder(fix_phantom)
                        })
                        .update_typed(&turtle_finder, |turtle| turtle.update_remainder(fix_turtle));
                    for block_attached_finder in &block_attached_finders {
                        entity = entity.update_typed(block_attached_finder, |block_attached| {
                            block_attached.update_remainder(fix_block_attached)
                        });
                    }
                    entity
                },
            ),
        ])
    })
}

fn fix_player(tag: Dynamic) -> Dynamic {
    let mut tag = fix_living_entity(tag);
    let spawn_x = tag.get("SpawnX").as_number().result();
    let spawn_y = tag.get("SpawnY").as_number().result();
    let spawn_z = tag.get("SpawnZ").as_number().result();
    if let (Some(spawn_x), Some(spawn_y), Some(spawn_z)) = (spawn_x, spawn_y, spawn_z) {
        let mut respawn = tag.create_map([(
            tag.create_string("pos"),
            create_block_pos(
                &tag,
                spawn_x.int_value(),
                spawn_y.int_value(),
                spawn_z.int_value(),
            ),
        )]);
        respawn = Dynamic::copy_field(&tag, "SpawnAngle", &respawn, "angle");
        respawn = Dynamic::copy_field(&tag, "SpawnDimension", &respawn, "dimension");
        respawn = Dynamic::copy_field(&tag, "SpawnForced", &respawn, "forced");
        tag = tag
            .remove("SpawnX")
            .remove("SpawnY")
            .remove("SpawnZ")
            .remove("SpawnAngle")
            .remove("SpawnDimension")
            .remove("SpawnForced");
        tag = tag.set("respawn", respawn);
    }

    if let Some(entered_nether_pos) = tag.get("enteredNetherPosition").result() {
        tag = tag.remove("enteredNetherPosition").set(
            "entered_nether_pos",
            tag.create_list([
                tag.create_double(entered_nether_pos.get("x").as_double_or(0.0)),
                tag.create_double(entered_nether_pos.get("y").as_double_or(0.0)),
                tag.create_double(entered_nether_pos.get("z").as_double_or(0.0)),
            ]),
        );
    }

    tag
}

fn fix_living_entity(tag: Dynamic) -> Dynamic {
    fix_inline_block_pos(&tag, "SleepingX", "SleepingY", "SleepingZ", "sleeping_pos")
}

fn fix_vex(tag: Dynamic) -> Dynamic {
    fix_inline_block_pos(
        &tag.rename_field("LifeTicks", "life_ticks"),
        "BoundX",
        "BoundY",
        "BoundZ",
        "bound_pos",
    )
}

fn fix_phantom(tag: Dynamic) -> Dynamic {
    fix_inline_block_pos(
        &tag.rename_field("Size", "size"),
        "AX",
        "AY",
        "AZ",
        "anchor_pos",
    )
}

fn fix_turtle(tag: Dynamic) -> Dynamic {
    let tag = tag
        .remove("TravelPosX")
        .remove("TravelPosY")
        .remove("TravelPosZ");
    let tag = fix_inline_block_pos(&tag, "HomePosX", "HomePosY", "HomePosZ", "home_pos");
    tag.rename_field("HasEgg", "has_egg")
}

fn fix_block_attached(tag: Dynamic) -> Dynamic {
    fix_inline_block_pos(&tag, "TileX", "TileY", "TileZ", "block_pos")
}
