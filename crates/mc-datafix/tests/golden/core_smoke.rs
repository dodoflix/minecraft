//! Smoke tests of the type system / rewrite machinery with a tiny hand-written schema (no vanilla data).

use mc_datafix::*;
use mc_dfu::{Dyn, Dynamic, NBT};

struct Root;

impl SchemaDef for Root {
    fn register_entities(&self, _this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = TemplateMap::new();
        schema.register(&mut map, "Item", |s, _| {
            DSL::optional_fields(&[("Item", References::ITEM_STACK.in_(s))])
        });
        schema.register_simple(&mut map, "Zombie");
        schema.register_simple(&mut map, "Pig");
        map
    }
    fn register_block_entities(&self, _this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = TemplateMap::new();
        schema.register(&mut map, "Chest", |s, _| {
            DSL::optional_fields(&[("Items", DSL::list(&References::ITEM_STACK.in_(s)))])
        });
        map
    }
    fn register_types(
        &self,
        _this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        schema.register_type(false, &References::ITEM_NAME, |_| {
            DSL::const_type(&DSL::namespaced_string())
        });
        schema.register_type(true, &References::ITEM_STACK, |s| {
            DSL::optional_fields(&[
                ("id", References::ITEM_NAME.in_(s)),
                ("Contents", DSL::list(&References::ITEM_STACK.in_(s))),
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
            DSL::tagged_choice_lazy("id", &DSL::string(), &e, s)
        });
        let b = block_entity_types.clone();
        schema.register_type(false, &References::BLOCK_ENTITY, move |s| {
            DSL::tagged_choice_lazy("id", &DSL::string(), &b, s)
        });
        schema.register_type(false, &References::CHUNK, |s| {
            DSL::optional_fields(&[(
                "Level",
                DSL::optional_fields(&[
                    ("Entities", DSL::list(&References::ENTITY_TREE.in_(s))),
                    ("TileEntities", DSL::list(&References::BLOCK_ENTITY.in_(s))),
                ]),
            )])
        });
    }
}

fn s(v: &str) -> Dyn {
    Dyn::Str(v.to_string())
}

fn item(id: &str) -> Dyn {
    Dyn::map_of([("id", s(id)), ("Count", Dyn::Byte(1))])
}

fn chunk() -> Dyn {
    let zombie = |passengers: Vec<Dyn>| {
        let mut e = vec![("id", s("Zombie")), ("Health", Dyn::Int(20))];
        if !passengers.is_empty() {
            e.push(("Passengers", Dyn::List(passengers)));
        }
        Dyn::map_of(e)
    };
    let pig = Dyn::map_of([("id", s("Pig")), ("Health", Dyn::Int(10))]);
    let chest = Dyn::map_of([
        ("id", s("Chest")),
        (
            "Items",
            Dyn::List(vec![Dyn::map_of([
                ("id", s("minecraft:stone")),
                ("Contents", Dyn::List(vec![item("minecraft:apple")])),
            ])]),
        ),
    ]);
    Dyn::map_of([(
        "Level",
        Dyn::map_of([
            ("xPos", Dyn::Int(3)),
            (
                "Entities",
                Dyn::List(vec![zombie(vec![pig, zombie(vec![])]), zombie(vec![])]),
            ),
            ("TileEntities", Dyn::List(vec![chest])),
        ]),
    )])
}

fn builder_with_fix(mark: bool) -> DataFixer {
    let mut b = DataFixerBuilder::new(5023);
    b.add_schema(99, |k, p| Schema::new(k, p, Root));
    let v100 = b.add_schema(100, Schema::same);
    b.add_fixer(DataFix::new(&v100, false, move |fix| {
        let ty = fix.input_schema().get_type(&References::ENTITY);
        fix.fix_type_everywhere_typed("ZombieMark", &ty, move |typed| {
            typed.update_remainder(|mut d: Dynamic| {
                if mark && d.value.map_get("id").and_then(|i| i.as_str()) == Some("Zombie") {
                    if let Dyn::Map(m) = &mut d.value {
                        Dyn::map_put(m, s("marked"), Dyn::Byte(1));
                    }
                }
                d
            })
        })
    }));
    b.build()
}

fn count_marked(d: &Dyn) -> usize {
    match d {
        Dyn::Map(m) => m
            .iter()
            .map(|(k, v)| (k.as_str() == Some("marked")) as usize + count_marked(v))
            .sum(),
        Dyn::List(l) => l.iter().map(count_marked).sum(),
        _ => 0,
    }
}

#[test]
fn read_write_roundtrip() {
    let schema = Schema::new(990, None, Root);
    let ty = schema.get_type(&References::CHUNK);
    let input = Dynamic::new(NBT, chunk());
    let (typed, _) = ty.read_typed(&input).into_result().expect("decode");
    let out = typed.write().into_result().expect("encode");
    assert_eq!(out.value, input.value);
}

#[test]
fn everywhere_through_recursive_entities() {
    let fixer = builder_with_fix(true);
    let out = fixer.update(&References::CHUNK, Dynamic::new(NBT, chunk()), 99, 100);
    // 3 zombies in the tree (top, passenger-of-passenger, second top-level); the pig is untouched
    assert_eq!(count_marked(&out.value), 3, "{:?}", out.value);
    let unchanged = fixer.update(&References::CHUNK, Dynamic::new(NBT, chunk()), 100, 100);
    assert_eq!(unchanged.value, chunk());
}
