//! The abstract and family fix classes of `util/datafix/fixes` against the real vanilla classes
//! (`fixtures/base_fixes/BaseFixGolden.java`, regenerate with `fixtures/base_fixes/gen.sh`): every case registers one fix
//! at the second schema of a two-schema chain (the structure of the second schema changed by flags), updates one NBT
//! value from the first version to the second and compares the dumped result or the exception. The schemas and the
//! subclass bodies below are the same as in the Java generator.

use crate::library::{dump_dynamic, dyn_};
use mc_datafix::data_fixers::{create_renamer, create_renamer_no_namespace, create_renamer_pair};
use mc_datafix::fixes::{
    abstract_poi_section_fix, abstract_uuid_fix, add_field_fix, add_flag_if_not_present_fix,
    add_new_choices, advancements_rename_fix, attributes_rename_fix, block_entity_rename_fix,
    data_component_remainder_fix, entity_rename_fix, item_rename_fix, item_stack_tag_fix,
    item_stack_tag_remainder_fix, legacy_abstract_block_property_fix, legacy_block_rename_fix,
    named_entity_convert_unchecked_fix, named_entity_fix, named_entity_write_read_fix,
    namespaced_type_rename_fix, simple_entity_rename_fix, simplest_entity_rename_fix,
    write_and_read_fix,
};
use mc_datafix::*;
use mc_dfu::Dynamic;
use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

const GOLDEN: &str = include_str!("../fixtures/base_fixes.tsv");

type Flags = Arc<HashSet<String>>;

fn name_type(flags: &Flags) -> Type {
    if flags.contains("badnames") {
        DSL::string()
    } else {
        DSL::namespaced_string()
    }
}

/// The first schema (`S100` of the generator).
struct S100(Flags);

impl SchemaDef for S100 {
    fn namespaced(&self) -> bool {
        true
    }
    fn register_entities(&self, _this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = TemplateMap::new();
        schema.register_simple(&mut map, "minecraft:zombie");
        schema.register_simple(&mut map, "minecraft:pig");
        schema.register_simple(&mut map, "minecraft:cow");
        schema.register_with(&mut map, "minecraft:item", |s| {
            DSL::optional_fields(&[("Item", References::ITEM_STACK.in_(s))])
        });
        map
    }
    fn register_block_entities(&self, _this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = TemplateMap::new();
        schema.register_with(&mut map, "minecraft:chest", |s| {
            DSL::optional_fields(&[("Items", DSL::list(&References::ITEM_STACK.in_(s)))])
        });
        schema.register_simple(&mut map, "minecraft:sign");
        map
    }
    fn register_types(
        &self,
        _this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        for reference in [
            &References::ITEM_NAME,
            &References::BLOCK_NAME,
            &References::ENTITY_NAME,
            &References::BIOME,
        ] {
            let ty = name_type(&self.0);
            schema.register_type(false, reference, move |_| DSL::const_type(&ty));
        }
        schema.register_type(true, &References::ITEM_STACK, |s| {
            DSL::optional_fields(&[
                ("id", References::ITEM_NAME.in_(s)),
                (
                    "tag",
                    DSL::optional_fields(&[("Items", DSL::list(&References::ITEM_STACK.in_(s)))]),
                ),
            ])
        });
        schema.register_type(true, &References::ENTITY_TREE, |s| {
            DSL::optional_fields_with_rest(
                &[("Passengers", DSL::list(&References::ENTITY_TREE.in_(s)))],
                &References::ENTITY.in_(s),
            )
        });
        let e = entity_types.clone();
        schema.register_type(true, &References::ENTITY, move |s| {
            DSL::tagged_choice_lazy("id", &DSL::namespaced_string(), &e, s)
        });
        let b = block_entity_types.clone();
        schema.register_type(false, &References::BLOCK_ENTITY, move |s| {
            DSL::tagged_choice_lazy("id", &DSL::namespaced_string(), &b, s)
        });
        for reference in [
            &References::BLOCK_STATE,
            &References::FLAT_BLOCK_STATE,
            &References::DATA_COMPONENTS,
            &References::ADVANCEMENTS,
            &References::POI_CHUNK,
        ] {
            schema.register_type(false, reference, |_| DSL::remainder());
        }
        schema.register_type(false, &References::PLAYER, |s| {
            DSL::optional_fields(&[("Inventory", DSL::list(&References::ITEM_STACK.in_(s)))])
        });
    }
}

/// The second schema (`S101` of the generator).
struct S101(Flags);

impl SchemaDef for S101 {
    fn namespaced(&self) -> bool {
        true
    }
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        if self.0.contains("newentities") {
            schema.register_simple(&mut map, "minecraft:cattle");
            schema.register_simple(&mut map, "minecraft:sheep");
        }
        if self.0.contains("dropcow") {
            map.remove("minecraft:cow");
        }
        if self.0.contains("pigsaddle") {
            schema.register_with(&mut map, "minecraft:pig", |s| {
                DSL::optional_fields(&[("Saddle", References::ITEM_STACK.in_(s))])
            });
        }
        if self.0.contains("itemfield") {
            schema.register_with(&mut map, "minecraft:item", |s| {
                DSL::optional_fields(&[("ItemStack", References::ITEM_STACK.in_(s))])
            });
        }
        map
    }
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        if self.0.contains("newbes") {
            schema.register_simple(&mut map, "minecraft:hanging_sign");
        }
        if self.0.contains("dropsign") {
            map.remove("minecraft:sign");
        }
        map
    }
    fn register_types(
        &self,
        this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        this.super_register_types(schema, entity_types, block_entity_types);
        if self.0.contains("bekey") {
            let b = block_entity_types.clone();
            schema.register_type(false, &References::BLOCK_ENTITY, move |s| {
                DSL::tagged_choice_lazy("id", &DSL::string(), &b, s)
            });
        }
        if self.0.contains("badentityname") {
            schema.register_type(false, &References::ENTITY_NAME, |_| {
                DSL::const_type(&DSL::string())
            });
        }
        if self.0.contains("badpoi") {
            schema.register_type(false, &References::POI_CHUNK, |_| {
                DSL::optional_fields(&[("Sections", DSL::remainder())])
            });
        }
    }
}

fn mark(d: Dynamic) -> Dynamic {
    d.set("fixed", d.create_byte(1))
}

fn rename(from: &'static str, to: &'static str) -> impl Fn(&str) -> String + Send + Sync + 'static {
    move |s| (if s == from { to } else { s }).to_string()
}

/// `make(fix, v101)` of the generator.
fn make(fix: &str, v101: &Schema) -> DataFix {
    use References as R;
    match fix {
        "named_entity" => named_entity_fix::new(
            "NamedEntityFix",
            v101,
            false,
            "NamedZombie",
            &R::ENTITY,
            "minecraft:zombie",
            |entity| entity.update_remainder(mark),
        ),
        "named_entity_be" => named_entity_fix::new(
            "NamedEntityFix",
            v101,
            false,
            "NamedChest",
            &R::BLOCK_ENTITY,
            "minecraft:chest",
            |entity| entity.update_remainder(mark),
        ),
        "named_entity_missing" => named_entity_fix::new(
            "NamedEntityFix",
            v101,
            false,
            "NamedGhost",
            &R::ENTITY,
            "minecraft:ghost",
            |entity| entity,
        ),
        "named_entity_write_read" => named_entity_write_read_fix::new(
            "NamedEntityWriteReadFix",
            v101,
            true,
            "WriteReadPig",
            &R::ENTITY,
            "minecraft:pig",
            |input| input.set("wr", input.create_int(2)),
        ),
        "named_entity_convert" => named_entity_convert_unchecked_fix::new(
            v101,
            "ConvertItem",
            &R::ENTITY,
            "minecraft:item",
        ),
        "component_inc" => data_component_remainder_fix::new(
            "DataComponentRemainderFix",
            v101,
            "Inc",
            "minecraft:foo",
            increment,
        ),
        "component_rename" => data_component_remainder_fix::new_renamed(
            "DataComponentRemainderFix",
            v101,
            "IncRename",
            "minecraft:foo",
            "minecraft:bar",
            increment,
        ),
        "item_stack_tag" => item_stack_tag_fix::new(
            "ItemStackTagFix",
            v101,
            "TagStone",
            |id| id == "minecraft:stone",
            |tag| tag.update_remainder(mark),
        ),
        "item_stack_tag_remainder" => item_stack_tag_remainder_fix::new(
            "ItemStackTagRemainderFix",
            v101,
            "TagS",
            |id| id.starts_with("minecraft:s"),
            |tag| tag.set("rem", tag.create_string("y")),
        ),
        "uuid" => DataFix::new("AbstractUUIDFix", v101, false, |fix| {
            let schema = fix.input_schema();
            let entity_type = schema.get_type(&R::ENTITY);
            fix.fix_type_everywhere_typed("UuidTest", &entity_type, move |typed| {
                abstract_uuid_fix::update_named_choice(
                    &schema,
                    &R::ENTITY,
                    typed,
                    "minecraft:zombie",
                    |d| {
                        let mut out = d;
                        out =
                            abstract_uuid_fix::replace_uuid_string(&out, "S", "S2").unwrap_or(out);
                        out = abstract_uuid_fix::replace_uuid_ml_tag(&out, "ML", "ML2")
                            .unwrap_or(out);
                        abstract_uuid_fix::replace_uuid_least_most(&out, "LM", "LM2").unwrap_or(out)
                    },
                )
            })
        }),
        "poi" => {
            abstract_poi_section_fix::new("AbstractPoiSectionFix", v101, "PoiFilter", |records| {
                records
                    .into_iter()
                    .filter(|r| r.get("type").as_string_or("") != "drop")
                    .map(|r| r.set("seen", r.create_byte(1)))
                    .collect()
            })
        }
        "entity_rename" => entity_rename_fix::new(
            "EntityRenameFix",
            "RenameCow",
            v101,
            true,
            |name, entity| (rename("minecraft:cow", "minecraft:cattle")(name), entity),
        ),
        "entity_rename_pig" => entity_rename_fix::new(
            "EntityRenameFix",
            "RenameCowToPig",
            v101,
            true,
            |name, entity| (rename("minecraft:cow", "minecraft:pig")(name), entity),
        ),
        "entity_rename_ghost" => entity_rename_fix::new(
            "EntityRenameFix",
            "RenameCowToGhost",
            v101,
            true,
            |name, entity| (rename("minecraft:cow", "minecraft:ghost")(name), entity),
        ),
        "simple_entity_rename" => simple_entity_rename_fix::new(
            "SimpleEntityRenameFix",
            "SimpleRename",
            v101,
            true,
            |name, tag| {
                if name == "minecraft:cow" {
                    (
                        "minecraft:cattle".to_string(),
                        tag.set("renamed", tag.create_byte(1)),
                    )
                } else {
                    (name.to_string(), tag)
                }
            },
        ),
        "simplest_entity_rename" => simplest_entity_rename_fix::new(
            "SimplestEntityRenameFix",
            "SimplestRename",
            v101,
            true,
            rename("minecraft:cow", "minecraft:cattle"),
        ),
        "simplest_entity_rename_ghost" => simplest_entity_rename_fix::new(
            "SimplestEntityRenameFix",
            "SimplestRenameGhost",
            v101,
            true,
            rename("minecraft:cow", "minecraft:ghost"),
        ),
        "attributes_rename" => attributes_rename_fix::new(
            "AttributesRenameFix",
            v101,
            "AttrRename",
            rename("minecraft:generic.foo", "minecraft:foo"),
        ),
        "attributes_rename_old" => attributes_rename_fix::new_with_format(
            "AttributesRenameFix",
            v101,
            "AttrRenameOld",
            rename("minecraft:generic.foo", "minecraft:foo"),
            true,
        ),
        "legacy_block_property" => legacy_abstract_block_property_fix::new(
            "LegacyAbstractBlockPropertyFix",
            v101,
            "BlockProp",
            |id| id == "minecraft:stone",
            |id, properties| properties.set("fixed", properties.create_string(id)),
        ),
        "item_rename" => item_rename_fix::create(
            v101,
            "ItemRename",
            rename("minecraft:old_item", "minecraft:new_item"),
        ),
        "legacy_block_rename" => legacy_block_rename_fix::create(
            v101,
            "BlockRename",
            rename("minecraft:old", "minecraft:new"),
        ),
        "namespaced_type_rename" => namespaced_type_rename_fix::new(
            v101,
            "BiomeRename",
            &R::BIOME,
            rename("minecraft:old_biome", "minecraft:new_biome"),
        ),
        "block_entity_rename" => block_entity_rename_fix::create(
            v101,
            "BlockEntityRename",
            rename("minecraft:sign", "minecraft:hanging_sign"),
        ),
        "advancements_rename" => advancements_rename_fix::new(
            "AdvancementsRenameFix",
            v101,
            false,
            "AdvRename",
            rename("minecraft:a", "minecraft:b"),
        ),
        "advancements_rename_changes" => advancements_rename_fix::new(
            "AdvancementsRenameFix",
            v101,
            true,
            "AdvRenameChanges",
            rename("minecraft:a", "minecraft:b"),
        ),
        "advancements_rename_all" => advancements_rename_fix::new(
            "AdvancementsRenameFix",
            v101,
            false,
            "AdvRenameAll",
            |_| "minecraft:z".to_string(),
        ),
        "write_and_read" => write_and_read_fix::new(v101, "WriteAndRead", &R::ENTITY),
        "add_new_choices_entity" => add_new_choices::new(v101, "AddChoices", &R::ENTITY),
        "add_new_choices_block_entity" => {
            add_new_choices::new(v101, "AddChoices", &R::BLOCK_ENTITY)
        }
        "add_field" => add_field_fix::new(v101, &R::PLAYER, "added", |d| d.create_int(7), &[]),
        "add_field_path" => add_field_fix::new(
            v101,
            &R::PLAYER,
            "added",
            |d| d.create_int(7),
            &["Inner", "Deep"],
        ),
        "add_flag" => add_flag_if_not_present_fix::new(v101, &R::PLAYER, "flag", true),
        other => panic!("unknown fix {other}"),
    }
}

/// `fixComponent` of the generator: the number plus one, anything else drops the component (Java `null`).
fn increment(input: Dynamic) -> Option<Dynamic> {
    let n = input.as_number().result()?;
    Some(input.create_int(n.int_value().wrapping_add(1)))
}

/// `EX:<simple class>:<message>` of a panic raised as `java.lang.X: message`; anything else is `EX:panic:..`.
fn thrown(payload: &(dyn std::any::Any + Send)) -> String {
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("");
    match message.split_once(": ") {
        Some((class, rest))
            if (class.starts_with("java.") || class.starts_with("net.minecraft.")) =>
        {
            let simple = class.rsplit('.').next().unwrap();
            format!("EX:{simple}:{rest}")
        }
        _ => format!("EX:panic:{message}"),
    }
}

/// The `createRenamer*` helpers of `DataFixers` over the map of the generator.
fn rename_with(kind: &str, id: &str) -> String {
    let map = [("minecraft:a", "minecraft:x"), ("b", "y")];
    match kind {
        "renamer_map" => create_renamer(&map)(id),
        "renamer_no_namespace" => create_renamer_no_namespace(&map)(id),
        "renamer_pair" => create_renamer_pair("minecraft:a", "minecraft:x")(id),
        other => panic!("unknown renamer {other}"),
    }
}

fn run(fix: &str, flags: &str, reference: &str, input: &str) -> String {
    let flags: Flags = Arc::new(flags.split(',').map(str::to_string).collect());
    let reference = References::by_name(reference).expect("known type");
    catch_unwind(AssertUnwindSafe(|| {
        let mut builder = DataFixerBuilder::new(5023);
        let f = flags.clone();
        builder.add_schema(100, move |k, p| Schema::new(k, p, S100(f)));
        let f = flags.clone();
        let v101 = builder.add_schema(101, move |k, p| Schema::new(k, p, S101(f)));
        builder.add_fixer(make(fix, &v101));
        dump_dynamic(&builder.build().update(&reference, dyn_(input), 100, 101))
    }))
    .unwrap_or_else(|payload| thrown(&*payload))
}

#[test]
fn base_fixes_equal_vanilla() {
    let mut failures = Vec::new();
    let mut total = 0;
    for line in GOLDEN.lines() {
        let [fix, flags, reference, input, expected] = line.split('\t').collect::<Vec<_>>()[..]
        else {
            panic!("malformed row: {line}");
        };
        total += 1;
        let ours = if fix.starts_with("renamer_") {
            rename_with(fix, input)
        } else {
            run(fix, flags, reference, input)
        };
        if ours != expected {
            failures.push(format!(
                "{fix} [{flags}] {reference} {input}\n   vanilla: {expected}\n   ours:    {ours}"
            ));
        }
    }
    assert!(total > 150, "golden has {total} rows");
    assert!(
        failures.is_empty(),
        "{} of {total} differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
