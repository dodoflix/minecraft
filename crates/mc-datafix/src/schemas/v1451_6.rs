//! `net.minecraft.util.datafix.schemas.V1451_6`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};
use crate::ty::HookFunction;
use mc_core::ident::Identifier;
use mc_dfu::Dynamic;
use std::sync::LazyLock;

/// `V1451_6.SPECIAL_OBJECTIVE_MARKER`.
pub(crate) const SPECIAL_OBJECTIVE_MARKER: &str = "_special";

/// `V1451_6.UNPACK_OBJECTIVE_ID`: splits `CriteriaName` into `CriteriaType {type, id}`.
pub(crate) static UNPACK_OBJECTIVE_ID: LazyLock<HookFunction> = LazyLock::new(|| {
    HookFunction::new(|ops, value| {
        let input = Dynamic::new(*ops, value);
        let exploded_id = input.get("CriteriaName").as_string().result().map(|name| {
            let Some(colon_pos) = name.find(':') else {
                return (SPECIAL_OBJECTIVE_MARKER.to_string(), name);
            };
            match (
                Identifier::by_separator(&name[..colon_pos], '.'),
                Identifier::by_separator(&name[colon_pos + 1..], '.'),
            ) {
                (Ok(stat_type), Ok(stat_id)) => (stat_type.to_string(), stat_id.to_string()),
                _ => (SPECIAL_OBJECTIVE_MARKER.to_string(), name),
            }
        });
        match exploded_id {
            Some((stat_type, stat_id)) => input
                .set(
                    "CriteriaType",
                    input.create_map([
                        (input.create_string("type"), input.create_string(stat_type)),
                        (input.create_string("id"), input.create_string(stat_id)),
                    ]),
                )
                .into_value(),
            None => input.into_value(),
        }
    })
});

/// `V1451_6.REPACK_OBJECTIVE_ID`: the inverse of [`UNPACK_OBJECTIVE_ID`].
pub(crate) static REPACK_OBJECTIVE_ID: LazyLock<HookFunction> = LazyLock::new(|| {
    HookFunction::new(|ops, value| {
        let input = Dynamic::new(*ops, value);
        let repacked_id = input.get("CriteriaType").result().and_then(|ty| {
            let stat_type = ty.get("type").as_string().result()?;
            let stat_id = ty.get("id").as_string().result()?;
            Some(if stat_type == SPECIAL_OBJECTIVE_MARKER {
                input.create_string(stat_id)
            } else {
                ty.create_string(format!(
                    "{}:{}",
                    pack_namespaced_with_dot(&stat_type),
                    pack_namespaced_with_dot(&stat_id)
                ))
            })
        });
        match repacked_id {
            Some(id) => input
                .set("CriteriaName", id)
                .remove("CriteriaType")
                .into_value(),
            None => input.into_value(),
        }
    })
});

/// `V1451_6.createCriterionTypes`.
pub(crate) fn create_criterion_types() -> TemplateMap {
    let mut criterion_types = TemplateMap::new();
    for (name, reference) in [
        ("minecraft:mined", References::BLOCK_NAME),
        ("minecraft:crafted", References::ITEM_NAME),
        ("minecraft:used", References::ITEM_NAME),
        ("minecraft:broken", References::ITEM_NAME),
        ("minecraft:picked_up", References::ITEM_NAME),
        ("minecraft:dropped", References::ITEM_NAME),
        ("minecraft:killed", References::ENTITY_NAME),
        ("minecraft:killed_by", References::ENTITY_NAME),
    ] {
        criterion_types.put(name, move |s| {
            DSL::optional_fields(&[("id", reference.in_(s))])
        });
    }
    criterion_types.put("minecraft:custom", |_| {
        DSL::optional_fields(&[("id", DSL::const_type(&DSL::namespaced_string()))])
    });
    criterion_types.put("_special", |_| {
        DSL::optional_fields(&[("id", DSL::const_type(&DSL::string()))])
    });
    criterion_types
}

/// `V1451_6.packNamespacedWithDot`.
pub(crate) fn pack_namespaced_with_dot(location: &str) -> String {
    match Identifier::try_parse(location) {
        Some(parsed) => format!("{}.{}", parsed.namespace(), parsed.path()),
        None => location.to_string(),
    }
}
pub struct V1451_6;

impl SchemaDef for V1451_6 {
    fn namespaced(&self) -> bool {
        true
    }

    fn register_types(
        &self,
        this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        this.super_register_types(schema, entity_types, block_entity_types);
        let item_stats = |s: &Schema| {
            DSL::compound_list_kv(
                &References::ITEM_NAME.in_(s),
                &DSL::const_type(&DSL::int_type()),
            )
        };
        schema.register_type(false, &References::STATS, move |s| {
            DSL::optional_fields(&[(
                "stats",
                DSL::optional_fields(&[
                    (
                        "minecraft:mined",
                        DSL::compound_list_kv(
                            &References::BLOCK_NAME.in_(s),
                            &DSL::const_type(&DSL::int_type()),
                        ),
                    ),
                    ("minecraft:crafted", item_stats(s)),
                    ("minecraft:used", item_stats(s)),
                    ("minecraft:broken", item_stats(s)),
                    ("minecraft:picked_up", item_stats(s)),
                    ("minecraft:dropped", item_stats(s)),
                    (
                        "minecraft:killed",
                        DSL::compound_list_kv(
                            &References::ENTITY_NAME.in_(s),
                            &DSL::const_type(&DSL::int_type()),
                        ),
                    ),
                    (
                        "minecraft:killed_by",
                        DSL::compound_list_kv(
                            &References::ENTITY_NAME.in_(s),
                            &DSL::const_type(&DSL::int_type()),
                        ),
                    ),
                    (
                        "minecraft:custom",
                        DSL::compound_list_kv(
                            &DSL::const_type(&DSL::namespaced_string()),
                            &DSL::const_type(&DSL::int_type()),
                        ),
                    ),
                ]),
            )])
        });
        let criterion_types = create_criterion_types();
        schema.register_type(false, &References::OBJECTIVE, move |s| {
            DSL::hook(
                &DSL::optional_fields(&[
                    (
                        "CriteriaType",
                        DSL::tagged_choice_lazy("type", &DSL::string(), &criterion_types, s),
                    ),
                    ("DisplayName", References::TEXT_COMPONENT.in_(s)),
                ]),
                &UNPACK_OBJECTIVE_ID,
                &REPACK_OBJECTIVE_ID,
            )
        });
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1451_6)
}
