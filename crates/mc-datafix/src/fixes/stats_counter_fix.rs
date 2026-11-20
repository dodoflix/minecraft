//! `StatsCounterFix`: converts the flat legacy statistics (`stat.walkOneCm`, `stat.mineBlock.minecraft.stone`) and
//! scoreboard objective criteria to the typed `stats` structure.

use crate::datafix::DataFix;
use crate::finder::remainder_finder;
use crate::fixes::block_state_data;
use crate::fixes::references::References;
use crate::fixes::tables::{OrderedMap, OrderedSet, item_stack_the_flattening_fix, table};
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use crate::schemas::v1451_6::pack_namespaced_with_dot;
use crate::util::read_typed_or_throw;
use mc_core::java_collections::{HASH_MAP_DEFAULT_CAPACITY, hash_map_ordered};
use mc_core::java_string::JavaString;
use mc_dfu::Dynamic;
use std::sync::LazyLock;

struct Tables {
    special_objective_criteria: OrderedSet<String>,
    skip: OrderedSet<String>,
    custom_map: OrderedMap<String, String>,
    item_keys: OrderedMap<String, String>,
    entity_keys: OrderedMap<String, String>,
    entities: OrderedMap<String, String>,
}

static TABLES: LazyLock<Tables> = LazyLock::new(|| {
    let t = table("stats_counter_fix");
    Tables {
        special_objective_criteria: t.field("SPECIAL_OBJECTIVE_CRITERIA").string_set(),
        skip: t.field("SKIP").string_set(),
        custom_map: t.field("CUSTOM_MAP").string_map(),
        item_keys: t.field("ITEM_KEYS").string_map(),
        entity_keys: t.field("ENTITY_KEYS").string_map(),
        entities: t.field("ENTITIES").string_map(),
    }
});

const BLOCK_KEY: &str = "stat.mineBlock";
const NEW_BLOCK_KEY: &str = "minecraft:mined";
const NEW_CUSTOM_KEY: &str = "minecraft:custom";

/// `StatsCounterFix.StatType`.
struct StatType {
    ty: String,
    type_key: JavaString,
}

impl StatType {
    fn new(ty: &str, type_key: impl Into<JavaString>) -> StatType {
        StatType {
            ty: ty.to_string(),
            type_key: type_key.into(),
        }
    }
}

/// `new StatsCounterFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("StatsCounterFix", output_schema, changes_type, |df| {
        TypeRewriteRule::seq(make_stat_fixer(df), make_objective_fixer(df))
    })
}

fn unpack_legacy_key(key: &JavaString) -> Option<StatType> {
    let t = &*TABLES;
    // a table key never holds a lone surrogate, so the text decides every lookup; the rest of the key keeps its units
    let text = key.as_lossy_str();
    if t.skip.contains(text) {
        return None;
    }
    if let Some(custom_key) = t.custom_map.get(text) {
        return Some(StatType::new(NEW_CUSTOM_KEY, custom_key.as_str()));
    }
    // `StringUtils.ordinalIndexOf(key, ".", 2)`
    let split_index = text.match_indices('.').nth(1)?.0;
    let prefix = &text[..split_index];
    let rest_start = text[..=split_index].encode_utf16().count();
    let rest = key
        .substring(rest_start, key.utf16_len())
        .map_lossless(|r| r.replace('.', ":"));
    if prefix == BLOCK_KEY {
        let new_key = rest.map_lossless(|r| block_state_data::upgrade_block(r).to_owned());
        return Some(StatType::new(NEW_BLOCK_KEY, new_key));
    }
    if let Some(item_key) = t.item_keys.get(prefix) {
        let new_key = rest.map_lossless(|r| {
            item_stack_the_flattening_fix::update_item(r, 0)
                .unwrap_or(r)
                .to_owned()
        });
        return Some(StatType::new(item_key, new_key));
    }
    if let Some(entity_key) = t.entity_keys.get(prefix) {
        let new_key =
            rest.map_lossless(|r| t.entities.get(r).cloned().unwrap_or_else(|| r.to_owned()));
        return Some(StatType::new(entity_key, new_key));
    }
    None
}

fn make_stat_fixer(df: &DataFix) -> TypeRewriteRule {
    let input_type = df.input_schema().get_type(&References::STATS);
    let output_type = df.output_schema().get_type(&References::STATS);
    let read_type = output_type.clone();
    df.fix_type_everywhere_typed_new("StatsCounterFix", &input_type, &output_type, move |input| {
        let tag = input.get(&remainder_finder()).into_dynamic();
        // a `HashMap<Dynamic, Dynamic>`: entries in `put` order, reordered below
        let mut stats: Vec<(Dynamic, Dynamic)> = Vec::new();
        if let Some(map) = tag.get_map_values().result() {
            for (entry_key, entry_value) in map {
                if entry_value.as_number().result().is_some() {
                    let key = entry_key.as_string_or("");
                    if let Some(stat_type) = unpack_legacy_key(&key) {
                        let new_type_key = tag.create_string(stat_type.ty);
                        let index = match stats.iter().position(|(k, _)| *k == new_type_key) {
                            Some(index) => index,
                            None => {
                                stats.push((new_type_key, tag.empty_map()));
                                stats.len() - 1
                            }
                        };
                        stats[index].1 = stats[index].1.set(&stat_type.type_key, entry_value);
                    }
                }
            }
        }
        let stats = hash_map_ordered(stats, |(k, _)| k.hash_code(), HASH_MAP_DEFAULT_CAPACITY);
        read_typed_or_throw(
            &read_type,
            &tag.empty_map().set("stats", tag.create_map(stats)),
            false,
        )
    })
}

fn make_objective_fixer(df: &DataFix) -> TypeRewriteRule {
    let input_type = df.input_schema().get_type(&References::OBJECTIVE);
    let output_type = df.output_schema().get_type(&References::OBJECTIVE);
    let read_type = output_type.clone();
    df.fix_type_everywhere_typed_new(
        "ObjectiveStatFix",
        &input_type,
        &output_type,
        move |input| {
            let tag = input.get(&remainder_finder()).into_dynamic();
            let updated_tag = tag.update("CriteriaName", |name| match name.as_string().result() {
                Some(key) => {
                    let new_key = if TABLES
                        .special_objective_criteria
                        .contains(key.as_lossy_str())
                    {
                        key
                    } else {
                        match unpack_legacy_key(&key) {
                            None => JavaString::from("dummy"),
                            Some(stat_type) => mc_core::jformat!(
                                "{}:{}",
                                pack_namespaced_with_dot(&stat_type.ty),
                                stat_type.type_key.map_lossless(pack_namespaced_with_dot)
                            ),
                        }
                    };
                    name.create_string(new_key)
                }
                None => name,
            });
            read_typed_or_throw(&read_type, &updated_tag, false)
        },
    )
}
