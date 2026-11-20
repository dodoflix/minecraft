//! `AttributeModifierIdFix`: gives attribute modifiers string ids instead of UUIDs (item components, entities, players).

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use crate::typed::Typed;
use mc_core::uuid_util::{
    least_significant_bits, most_significant_bits, uuid_from_msb_lsb, uuid_from_string,
};
use mc_dfu::Dynamic;
use std::sync::LazyLock;

/// The `UUID.fromString(..)` keys and ids of `ID_MAP`, in `ImmutableMap` insertion order (the map is only looked up).
const ID_TABLE: [(&str, &str); 40] = [
    (
        "736565d2-e1a7-403d-a3f8-1aeb3e302542",
        "minecraft:creative_mode_block_range",
    ),
    (
        "98491ef6-97b1-4584-ae82-71a8cc85cf73",
        "minecraft:creative_mode_entity_range",
    ),
    (
        "91AEAA56-376B-4498-935B-2F7F68070635",
        "minecraft:effect.speed",
    ),
    (
        "7107DE5E-7CE8-4030-940E-514C1F160890",
        "minecraft:effect.slowness",
    ),
    (
        "AF8B6E3F-3328-4C0A-AA36-5BA2BB9DBEF3",
        "minecraft:effect.haste",
    ),
    (
        "55FCED67-E92A-486E-9800-B47F202C4386",
        "minecraft:effect.mining_fatigue",
    ),
    (
        "648D7064-6A60-4F59-8ABE-C2C23A6DD7A9",
        "minecraft:effect.strength",
    ),
    (
        "C0105BF3-AEF8-46B0-9EBC-92943757CCBE",
        "minecraft:effect.jump_boost",
    ),
    (
        "22653B89-116E-49DC-9B6B-9971489B5BE5",
        "minecraft:effect.weakness",
    ),
    (
        "5D6F0BA2-1186-46AC-B896-C61C5CEE99CC",
        "minecraft:effect.health_boost",
    ),
    (
        "EAE29CF0-701E-4ED6-883A-96F798F3DAB5",
        "minecraft:effect.absorption",
    ),
    (
        "03C3C89D-7037-4B42-869F-B146BCB64D2E",
        "minecraft:effect.luck",
    ),
    (
        "CC5AF142-2BD2-4215-B636-2605AED11727",
        "minecraft:effect.unluck",
    ),
    ("6555be74-63b3-41f1-a245-77833b3c2562", "minecraft:evil"),
    (
        "1eaf83ff-7207-4596-b37a-d7a07b3ec4ce",
        "minecraft:powder_snow",
    ),
    (
        "662A6B8D-DA3E-4C1C-8813-96EA6097278D",
        "minecraft:sprinting",
    ),
    (
        "020E0DFB-87AE-4653-9556-831010E291A0",
        "minecraft:attacking",
    ),
    ("766bfa64-11f3-11ea-8d71-362b9e155667", "minecraft:baby"),
    ("7E0292F2-9434-48D5-A29F-9583AF7DF27F", "minecraft:covered"),
    (
        "9e362924-01de-4ddd-a2b2-d0f7a405a174",
        "minecraft:suffocating",
    ),
    ("5CD17E52-A79A-43D3-A529-90FDE04B181E", "minecraft:drinking"),
    ("B9766B59-9566-4402-BC1F-2EE2A276D836", "minecraft:baby"),
    (
        "49455A49-7EC5-45BA-B886-3B90B23A1718",
        "minecraft:attacking",
    ),
    (
        "845DB27C-C624-495F-8C9F-6020A9A58B6B",
        "minecraft:armor.boots",
    ),
    (
        "D8499B04-0E66-4726-AB29-64469D734E0D",
        "minecraft:armor.leggings",
    ),
    (
        "9F3D476D-C118-4544-8365-64846904B48E",
        "minecraft:armor.chestplate",
    ),
    (
        "2AD3F246-FEE1-4E67-B886-69FD380BB150",
        "minecraft:armor.helmet",
    ),
    (
        "C1C72771-8B8E-BA4A-ACE0-81A93C8928B2",
        "minecraft:armor.body",
    ),
    (
        "b572ecd2-ac0c-4071-abde-9594af072a37",
        "minecraft:enchantment.fire_protection",
    ),
    (
        "40a9968f-5c66-4e2f-b7f4-2ec2f4b3e450",
        "minecraft:enchantment.blast_protection",
    ),
    (
        "07a65791-f64d-4e79-86c7-f83932f007ec",
        "minecraft:enchantment.respiration",
    ),
    (
        "60b1b7db-fffd-4ad0-817c-d6c6a93d8a45",
        "minecraft:enchantment.aqua_affinity",
    ),
    (
        "11dc269a-4476-46c0-aff3-9e17d7eb6801",
        "minecraft:enchantment.depth_strider",
    ),
    (
        "87f46a96-686f-4796-b035-22e16ee9e038",
        "minecraft:enchantment.soul_speed",
    ),
    (
        "b9716dbd-50df-4080-850e-70347d24e687",
        "minecraft:enchantment.soul_speed",
    ),
    (
        "92437d00-c3a7-4f2e-8f6c-1f21585d5dd0",
        "minecraft:enchantment.swift_sneak",
    ),
    (
        "5d3d087b-debe-4037-b53e-d84f3ff51f17",
        "minecraft:enchantment.sweeping_edge",
    ),
    (
        "3ceb37c0-db62-46b5-bd02-785457b01d96",
        "minecraft:enchantment.efficiency",
    ),
    (
        "CB3F55D3-645C-4F38-A497-9C13A33DB5CF",
        "minecraft:base_attack_damage",
    ),
    (
        "FA233E1C-4180-4865-B01B-BCCE9785ACA3",
        "minecraft:base_attack_speed",
    ),
];

/// `ID_MAP` keyed by (most, least) significant bits.
static ID_MAP: LazyLock<Vec<((i64, i64), &'static str)>> = LazyLock::new(|| {
    ID_TABLE
        .iter()
        .map(|(uuid, id)| {
            let uuid = uuid_from_string(uuid)
                .unwrap_or_else(|e| panic!("java.lang.IllegalArgumentException: {e}"));
            (
                (most_significant_bits(&uuid), least_significant_bits(&uuid)),
                *id,
            )
        })
        .collect()
});

/// `NAME_MAP.get(name)`.
fn name_map(name: &str) -> Option<&'static str> {
    match name {
        "Random spawn bonus" => Some("minecraft:random_spawn_bonus"),
        "Random zombie-spawn bonus" => Some("minecraft:zombie_random_spawn_bonus"),
        "Leader zombie bonus" => Some("minecraft:leader_zombie_bonus"),
        "Zombie reinforcement callee charge" => Some("minecraft:reinforcement_callee_charge"),
        "Zombie reinforcement caller charge" => Some("minecraft:reinforcement_caller_charge"),
        _ => None,
    }
}

/// `new AttributeModifierIdFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("AttributeModifierIdFix", output_schema, false, |df| {
        let item_stack_type = df.input_schema().get_type(&References::ITEM_STACK);
        let components_finder = item_stack_type.find_field("components");
        let item_stack = df.fix_type_everywhere_typed(
            "AttributeIdFix (ItemStack)",
            &item_stack_type,
            move |item_stack| {
                item_stack.update_typed(&components_finder, |components| {
                    components.update_remainder(fix_item_stack_components)
                })
            },
        );
        let entity = df.fix_type_everywhere_typed(
            "AttributeIdFix (Entity)",
            &df.input_schema().get_type(&References::ENTITY),
            fix_entity,
        );
        let player = df.fix_type_everywhere_typed(
            "AttributeIdFix (Player)",
            &df.input_schema().get_type(&References::PLAYER),
            fix_entity,
        );
        TypeRewriteRule::seq_all(item_stack, vec![entity, player])
    })
}

/// `converted.put(key, value)` of an `Object2ObjectArrayMap`: replaces in place or appends.
fn put(converted: &mut Vec<(String, Dynamic)>, key: String, value: Dynamic) {
    match converted.iter_mut().find(|(k, _)| *k == key) {
        Some(entry) => entry.1 = value,
        None => converted.push((key, value)),
    }
}

fn fix_modifiers(modifiers: Vec<Dynamic>) -> Vec<Dynamic> {
    let mut converted: Vec<(String, Dynamic)> = Vec::new();
    for modifier in modifiers {
        let uuid = uuid_from_int_array(&modifier.get("uuid").as_int_stream());
        let name = modifier.get("name").as_string_or("");
        let id_from_uuid = uuid.and_then(|bits| {
            ID_MAP
                .iter()
                .find(|(key, _)| *key == bits)
                .map(|(_, id)| *id)
        });
        let id_from_name = name_map(name.as_lossy_str());
        if let Some(id_from_uuid) = id_from_uuid {
            let modifier = modifier.set("id", modifier.create_string(id_from_uuid));
            put(
                &mut converted,
                id_from_uuid.to_string(),
                modifier.remove("uuid").remove("name"),
            );
        } else if let Some(id_from_name) = id_from_name {
            let pre_existing = converted
                .iter()
                .find(|(k, _)| k == id_from_name)
                .map(|(_, v)| v.clone());
            match pre_existing {
                None => {
                    let modifier = modifier.set("id", modifier.create_string(id_from_name));
                    put(
                        &mut converted,
                        id_from_name.to_string(),
                        modifier.remove("uuid").remove("name"),
                    );
                }
                Some(pre_existing) => {
                    let amount = pre_existing.get("amount").as_double_or(0.0);
                    let added = modifier.get("amount").as_double_or(0.0);
                    put(
                        &mut converted,
                        id_from_name.to_string(),
                        pre_existing.set("amount", modifier.create_double(amount + added)),
                    );
                }
            }
        } else {
            let id = format!(
                "minecraft:{}",
                match uuid {
                    Some((most, least)) =>
                        uuid_from_msb_lsb(most, least).to_string().to_lowercase(),
                    None => "unknown".to_string(),
                }
            );
            let modifier = modifier.set("id", modifier.create_string(id.clone()));
            put(&mut converted, id, modifier.remove("uuid").remove("name"));
        }
    }
    converted.into_iter().map(|(_, v)| v).collect()
}

fn convert_modifier_for_entity(modifier: Dynamic) -> Dynamic {
    modifier
        .rename_field("UUID", "uuid")
        .rename_field("Name", "name")
        .rename_field("Amount", "amount")
        .rename_and_fix_field("Operation", "operation", |operation| {
            let name = match operation.as_int_or(0) {
                0 => "add_value",
                1 => "add_multiplied_base",
                2 => "add_multiplied_total",
                _ => "invalid",
            };
            operation.create_string(name)
        })
}

/// `DataFixUtils.orElse(list.asStreamOpt().result().map(fix).map(list::createList), list)`.
fn map_list(list: Dynamic, fix: impl FnOnce(Vec<Dynamic>) -> Vec<Dynamic>) -> Dynamic {
    match list.as_stream_opt().result() {
        Some(stream) => list.create_list(fix(stream)),
        None => list,
    }
}

fn fix_item_stack_components(components: Dynamic) -> Dynamic {
    components.update("minecraft:attribute_modifiers", |attribute_modifiers| {
        attribute_modifiers.update("modifiers", |modifiers| map_list(modifiers, fix_modifiers))
    })
}

fn fix_attribute(attribute: Dynamic) -> Dynamic {
    attribute
        .rename_field("Name", "id")
        .rename_field("Base", "base")
        .rename_and_fix_field("Modifiers", "modifiers", |modifiers| {
            map_list(modifiers, |stream| {
                fix_modifiers(
                    stream
                        .into_iter()
                        .map(convert_modifier_for_entity)
                        .collect(),
                )
            })
        })
}

fn fix_entity(entity: Typed) -> Typed {
    entity.update_remainder(|tag| {
        tag.rename_and_fix_field("Attributes", "attributes", |attribute_list| {
            map_list(attribute_list, |stream| {
                stream.into_iter().map(fix_attribute).collect()
            })
        })
    })
}

/// `uuidFromIntArray`: the (most, least) significant bits of the UUID, `None` unless there are exactly 4 ints.
pub fn uuid_from_int_array(int_array: &[i32]) -> Option<(i64, i64)> {
    let &[a, b, c, d] = int_array else {
        return None;
    };
    Some((
        i64::from(a) << 32 | i64::from(b as u32),
        i64::from(c) << 32 | i64::from(d as u32),
    ))
}
