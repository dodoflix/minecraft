//! `MobEffectIdFix`: replaces numeric mob effect ids by their string ids in block entities, entities, players and items.

use crate::datafix::DataFix;
use crate::dsl::{DSL, TypeReference};
use crate::fixes::references::References;
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use crate::typed::Typed;
use mc_dfu::Dynamic;

/// `ID_MAP.get(id)`.
fn id_map(id: i32) -> Option<&'static str> {
    Some(match id {
        1 => "minecraft:speed",
        2 => "minecraft:slowness",
        3 => "minecraft:haste",
        4 => "minecraft:mining_fatigue",
        5 => "minecraft:strength",
        6 => "minecraft:instant_health",
        7 => "minecraft:instant_damage",
        8 => "minecraft:jump_boost",
        9 => "minecraft:nausea",
        10 => "minecraft:regeneration",
        11 => "minecraft:resistance",
        12 => "minecraft:fire_resistance",
        13 => "minecraft:water_breathing",
        14 => "minecraft:invisibility",
        15 => "minecraft:blindness",
        16 => "minecraft:night_vision",
        17 => "minecraft:hunger",
        18 => "minecraft:weakness",
        19 => "minecraft:poison",
        20 => "minecraft:wither",
        21 => "minecraft:health_boost",
        22 => "minecraft:absorption",
        23 => "minecraft:saturation",
        24 => "minecraft:glowing",
        25 => "minecraft:levitation",
        26 => "minecraft:luck",
        27 => "minecraft:unluck",
        28 => "minecraft:slow_falling",
        29 => "minecraft:conduit_power",
        30 => "minecraft:dolphins_grace",
        31 => "minecraft:bad_omen",
        32 => "minecraft:hero_of_the_village",
        33 => "minecraft:darkness",
        _ => return None,
    })
}

const MOB_EFFECT_INSTANCE_CARRIER_ITEMS: [&str; 4] = [
    "minecraft:potion",
    "minecraft:splash_potion",
    "minecraft:lingering_potion",
    "minecraft:tipped_arrow",
];

/// `new MobEffectIdFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("MobEffectIdFix", output_schema, false, |df| {
        TypeRewriteRule::seq_all(
            block_entity_fixer(df),
            vec![entity_fixer(df), player_fixer(df), item_stack_fixer(df)],
        )
    })
}

fn get_and_convert_mob_effect_id(obj: &Dynamic, field_name: &str) -> Option<Dynamic> {
    obj.get(field_name)
        .as_number()
        .result()
        .and_then(|id| id_map(id.int_value()))
        .map(|id| obj.create_string(id))
}

fn update_mob_effect_id_field_to(
    input: &Dynamic,
    old_field_name: &str,
    output: &Dynamic,
    new_field_name: &str,
) -> Dynamic {
    let mapped_id = get_and_convert_mob_effect_id(input, old_field_name);
    output.replace_field(old_field_name, new_field_name, mapped_id)
}

fn update_mob_effect_id_field(
    input: &Dynamic,
    old_field_name: &str,
    new_field_name: &str,
) -> Dynamic {
    update_mob_effect_id_field_to(input, old_field_name, input, new_field_name)
}

fn update_mob_effect_instance(input: Dynamic) -> Dynamic {
    let input = update_mob_effect_id_field(&input, "Id", "id");
    let input = input.rename_field("Ambient", "ambient");
    let input = input.rename_field("Amplifier", "amplifier");
    let input = input.rename_field("Duration", "duration");
    let input = input.rename_field("ShowParticles", "show_particles");
    let input = input.rename_field("ShowIcon", "show_icon");
    let hidden_effect = input
        .get("HiddenEffect")
        .result()
        .map(update_mob_effect_instance);
    input.replace_field("HiddenEffect", "hidden_effect", hidden_effect)
}

fn update_mob_effect_instance_list(input: &Dynamic, old_field: &str, new_field: &str) -> Dynamic {
    let new_value = input
        .get(old_field)
        .as_stream_opt()
        .result()
        .map(|effects| input.create_list(effects.into_iter().map(update_mob_effect_instance)));
    input.replace_field(old_field, new_field, new_value)
}

fn update_suspicious_stew_entry_to(input: &Dynamic, output: Dynamic) -> Dynamic {
    let output = update_mob_effect_id_field_to(input, "EffectId", &output, "id");
    let duration = input.get("EffectDuration").result();
    output.replace_field("EffectDuration", "duration", duration)
}

fn update_suspicious_stew_entry(input: Dynamic) -> Dynamic {
    update_suspicious_stew_entry_to(&input, input.clone())
}

fn update_named_choice(
    input: Typed,
    input_schema: &Schema,
    output_schema: &Schema,
    type_reference: &TypeReference,
    name: &str,
    function: fn(Dynamic) -> Dynamic,
) -> Typed {
    let old_type = input_schema.get_choice_type(type_reference, name);
    let new_type = output_schema.get_choice_type(type_reference, name);
    input.update_typed_as(
        &DSL::named_choice(name, &old_type),
        &new_type,
        |typed_tag| typed_tag.update_remainder(function),
    )
}

fn block_entity_fixer(df: &DataFix) -> TypeRewriteRule {
    let (input_schema, output_schema) = (df.input_schema(), df.output_schema());
    let block_entity_type = input_schema.get_type(&References::BLOCK_ENTITY);
    df.fix_type_everywhere_typed(
        "BlockEntityMobEffectIdFix",
        &block_entity_type,
        move |input| {
            update_named_choice(
                input,
                &input_schema,
                &output_schema,
                &References::BLOCK_ENTITY,
                "minecraft:beacon",
                |tag| {
                    let tag = update_mob_effect_id_field(&tag, "Primary", "primary_effect");
                    update_mob_effect_id_field(&tag, "Secondary", "secondary_effect")
                },
            )
        },
    )
}

fn fix_mooshroom_tag(entity_tag: Dynamic) -> Dynamic {
    let initial_entry = entity_tag.empty_map();
    let entry = update_suspicious_stew_entry_to(&entity_tag, initial_entry.clone());
    let entity_tag = if entry != initial_entry {
        entity_tag.set("stew_effects", entity_tag.create_list([entry]))
    } else {
        entity_tag
    };
    entity_tag.remove("EffectId").remove("EffectDuration")
}

fn fix_arrow_tag(data: Dynamic) -> Dynamic {
    update_mob_effect_instance_list(&data, "CustomPotionEffects", "custom_potion_effects")
}

fn fix_area_effect_cloud_tag(data: Dynamic) -> Dynamic {
    update_mob_effect_instance_list(&data, "Effects", "effects")
}

fn update_living_entity_tag(data: Dynamic) -> Dynamic {
    update_mob_effect_instance_list(&data, "ActiveEffects", "active_effects")
}

fn entity_fixer(df: &DataFix) -> TypeRewriteRule {
    let (input_schema, output_schema) = (df.input_schema(), df.output_schema());
    let entity_type = input_schema.get_type(&References::ENTITY);
    df.fix_type_everywhere_typed("EntityMobEffectIdFix", &entity_type, move |input| {
        let input = update_named_choice(
            input,
            &input_schema,
            &output_schema,
            &References::ENTITY,
            "minecraft:mooshroom",
            fix_mooshroom_tag,
        );
        let input = update_named_choice(
            input,
            &input_schema,
            &output_schema,
            &References::ENTITY,
            "minecraft:arrow",
            fix_arrow_tag,
        );
        let input = update_named_choice(
            input,
            &input_schema,
            &output_schema,
            &References::ENTITY,
            "minecraft:area_effect_cloud",
            fix_area_effect_cloud_tag,
        );
        input.update_remainder(update_living_entity_tag)
    })
}

fn player_fixer(df: &DataFix) -> TypeRewriteRule {
    let player_type = df.input_schema().get_type(&References::PLAYER);
    df.fix_type_everywhere_typed("PlayerMobEffectIdFix", &player_type, |input| {
        input.update_remainder(update_living_entity_tag)
    })
}

fn fix_suspicious_stew_tag(tag: Dynamic) -> Dynamic {
    let effects_list = tag
        .get("Effects")
        .as_stream_opt()
        .result()
        .map(|list| tag.create_list(list.into_iter().map(update_suspicious_stew_entry)));
    tag.replace_field("Effects", "effects", effects_list)
}

fn item_stack_fixer(df: &DataFix) -> TypeRewriteRule {
    let id_f = DSL::field_finder(
        "id",
        &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()),
    );
    let item_stack_type = df.input_schema().get_type(&References::ITEM_STACK);
    let tag_f = item_stack_type.find_field("tag");
    df.fix_type_everywhere_typed("ItemStackMobEffectIdFix", &item_stack_type, move |input| {
        if let Some(id_opt) = input.get_optional(&id_f) {
            let id = id_opt.second().as_str();
            if id == "minecraft:suspicious_stew" {
                return input.update_typed(&tag_f, |item_tag| {
                    item_tag.update_remainder(fix_suspicious_stew_tag)
                });
            }
            if MOB_EFFECT_INSTANCE_CARRIER_ITEMS.contains(&id) {
                return input.update_typed(&tag_f, |item_tag| {
                    item_tag.update_remainder(|tag| {
                        update_mob_effect_instance_list(
                            &tag,
                            "CustomPotionEffects",
                            "custom_potion_effects",
                        )
                    })
                });
            }
        }
        input
    })
}
