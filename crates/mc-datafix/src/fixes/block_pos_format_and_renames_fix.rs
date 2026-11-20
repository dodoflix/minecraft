//! `BlockPosFormatAndRenamesFix`: block positions of entities, block entities, map frames and compasses become int
//! lists and their fields take snake_case names.

use crate::datafix::DataFix;
use crate::dsl::{DSL, TypeReference};
use crate::extra_data_fix_utils::fix_block_pos;
use crate::fixes::item_stack_tag_fix;
use crate::fixes::references::References;
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use crate::typed::Typed;
use mc_dfu::Dynamic;

const PATROLLING_MOBS: [&str; 6] = [
    "minecraft:witch",
    "minecraft:ravager",
    "minecraft:pillager",
    "minecraft:illusioner",
    "minecraft:evoker",
    "minecraft:vindicator",
];

/// `new BlockPosFormatAndRenamesFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("BlockPosFormatAndRenamesFix", output_schema, true, |df| {
        let mut rules = Vec::new();
        add_entity_rules(df, &mut rules);
        add_block_entity_rules(df, &mut rules);
        rules.push(
            df.write_fix_and_read(
                "BlockPos format for map frames",
                &df.input_schema().get_type(&References::SAVED_DATA_MAP_DATA),
                &df.output_schema()
                    .get_type(&References::SAVED_DATA_MAP_DATA),
                |input| input.update("data", fix_map_saved_data),
            ),
        );
        let item_stack_type = df.input_schema().get_type(&References::ITEM_STACK);
        rules.push(df.fix_type_everywhere_typed(
            "BlockPos format for compass target",
            &item_stack_type,
            item_stack_tag_fix::create_fixer(
                &item_stack_type,
                |id| id == "minecraft:compass",
                |typed| {
                    typed.update_remainder(|tag| {
                        tag.update("LodestonePos", |pos| fix_block_pos(&pos))
                    })
                },
            ),
        ));
        TypeRewriteRule::seq_list(rules)
    })
}

fn fix_fields(typed: Typed, fields: &[(&'static str, &'static str)]) -> Typed {
    typed.update_remainder(|mut tag| {
        for (old, new) in fields {
            tag = tag.rename_and_fix_field(old, new, |pos| fix_block_pos(&pos));
        }
        tag
    })
}

fn fix_map_saved_data(data: Dynamic) -> Dynamic {
    data.update("frames", |frames| {
        let fixed: Vec<Dynamic> = frames
            .as_stream()
            .into_iter()
            .map(|frame| {
                let frame = frame.rename_and_fix_field("Pos", "pos", |pos| fix_block_pos(&pos));
                let frame = frame.rename_field("Rotation", "rotation");
                frame.rename_field("EntityId", "entity_id")
            })
            .collect();
        frames.create_list(fixed)
    })
    .update("banners", |banners| {
        let fixed: Vec<Dynamic> = banners
            .as_stream()
            .into_iter()
            .map(|banner| {
                let banner = banner.rename_field("Pos", "pos");
                let banner = banner.rename_field("Color", "color");
                banner.rename_field("Name", "name")
            })
            .collect();
        banners.create_list(fixed)
    })
}

fn add_entity_rules(df: &DataFix, rules: &mut Vec<TypeRewriteRule>) {
    let entity = &References::ENTITY;
    rules.push(create_entity_fixer(
        df,
        entity,
        "minecraft:bee",
        &[("HivePos", "hive_pos"), ("FlowerPos", "flower_pos")],
    ));
    rules.push(create_entity_fixer(
        df,
        entity,
        "minecraft:end_crystal",
        &[("BeamTarget", "beam_target")],
    ));
    rules.push(create_entity_fixer(
        df,
        entity,
        "minecraft:wandering_trader",
        &[("WanderTarget", "wander_target")],
    ));
    for patrolling_mob in PATROLLING_MOBS {
        rules.push(create_entity_fixer(
            df,
            entity,
            patrolling_mob,
            &[("PatrolTarget", "patrol_target")],
        ));
    }
    rules.push(df.fix_type_everywhere_typed(
        "BlockPos format in Leash for mobs",
        &df.input_schema().get_type(entity),
        |input| {
            input.update_remainder(|tag| {
                tag.rename_and_fix_field("Leash", "leash", |pos| fix_block_pos(&pos))
            })
        },
    ));
}

fn add_block_entity_rules(df: &DataFix, rules: &mut Vec<TypeRewriteRule>) {
    let block_entity = &References::BLOCK_ENTITY;
    rules.push(create_entity_fixer(
        df,
        block_entity,
        "minecraft:beehive",
        &[("FlowerPos", "flower_pos")],
    ));
    rules.push(create_entity_fixer(
        df,
        block_entity,
        "minecraft:end_gateway",
        &[("ExitPortal", "exit_portal")],
    ));
}

fn create_entity_fixer(
    df: &DataFix,
    ty: &TypeReference,
    entity_name: &str,
    fields: &'static [(&'static str, &'static str)],
) -> TypeRewriteRule {
    let keys: Vec<&str> = fields.iter().map(|(old, _)| *old).collect();
    let name = format!(
        "BlockPos format in [{}] for {entity_name} ({})",
        keys.join(", "),
        ty.type_name()
    );
    let entity_f = DSL::named_choice(
        entity_name,
        &df.input_schema().get_choice_type(ty, entity_name),
    );
    df.fix_type_everywhere_typed(&name, &df.input_schema().get_type(ty), move |input| {
        input.update_typed(&entity_f, |entity| fix_fields(entity, fields))
    })
}
