//! Smoke tests of the type system / rewrite machinery with a tiny hand-written schema (no vanilla data).

use mc_datafix::*;
use mc_dfu::{Dyn, Dynamic, NBT, PASSTHROUGH};
use mc_nbt::Tag;
use std::cell::RefCell;
use std::sync::{LazyLock, Once};

/// `Hook.HookFunction`s are compared by identity: one value per constant.
static DROP_JUNK: LazyLock<HookFunction> = LazyLock::new(|| {
    HookFunction::new(|ops, v| {
        let ops = *ops;
        ops.remove(&v, "junk")
    })
});
static ADD_STAMP: LazyLock<HookFunction> = LazyLock::new(|| {
    HookFunction::new(|ops, v| {
        let ops = *ops;
        ops.set(&v, "stamp", ops.create_int(9))
    })
});

thread_local! {
    static LOGGED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Records the records logged by the current thread.
struct CaptureLogger;

impl log::Log for CaptureLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        LOGGED.with(|l| {
            l.borrow_mut()
                .push(format!("{}: {}", record.level(), record.args()))
        });
    }
    fn flush(&self) {}
}

fn capture_logs<R>(f: impl FnOnce() -> R) -> (R, Vec<String>) {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        log::set_logger(&CaptureLogger).expect("no other logger in this test binary");
        log::set_max_level(log::LevelFilter::Trace);
    });
    LOGGED.with(|l| l.borrow_mut().clear());
    let result = f();
    (
        result,
        LOGGED.with(|l| std::mem::take(&mut *l.borrow_mut())),
    )
}

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
        schema.register_type(false, &References::HOTBAR, |_| {
            DSL::hook(
                &DSL::compound_list(&DSL::const_type(&DSL::int_type())),
                &DROP_JUNK,
                &ADD_STAMP,
            )
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
    b.add_fixer(DataFix::new("ZombieMarkFix", &v100, false, move |fix| {
        let ty = fix.input_schema().get_type(&References::ENTITY);
        fix.fix_type_everywhere_typed("ZombieMark", &ty, move |typed| {
            typed.update_remainder(|mut d: Dynamic| {
                if mark
                    && d.value.map_get("id").and_then(|i| i.as_str()) == Some("Zombie")
                    && let Dyn::Map(m) = &mut d.value
                {
                    Dyn::map_put(m, s("marked"), Dyn::Byte(1));
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
    // The NBT ops emit compound entries in Java `HashMap` order, `Dyn::Map` keeps insertion order.
    assert!(out.value.java_eq(&input.value, false), "{:?}", out.value);
}

fn leaked(fixer: DataFixer) -> &'static DataFixer {
    Box::leak(Box::new(fixer))
}

fn nbt_chunk() -> Dynamic {
    Dynamic::new(NBT, chunk())
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

#[test]
fn wrap_codec_encode_adds_data_version() {
    let codec = DataFixTypes::CHUNK.wrap_codec(&PASSTHROUGH, leaked(builder_with_fix(true)), 99);
    let encoded = codec
        .encode_start(&NBT, &nbt_chunk())
        .into_result()
        .expect("encode");
    assert_eq!(encoded.map_get("DataVersion"), Some(&Dyn::Int(5023)));
    assert!(encoded.map_get("Level").is_some());
}

#[test]
fn wrap_codec_decode_strips_the_version_and_fixes() {
    let codec = DataFixTypes::CHUNK.wrap_codec(&PASSTHROUGH, leaked(builder_with_fix(true)), 99);
    let with_version = |v: i32| {
        let Dyn::Map(mut m) = chunk() else {
            unreachable!()
        };
        Dyn::map_put(&mut m, s("DataVersion"), Dyn::Int(v));
        Dyn::Map(m)
    };
    // stored version 99: the fix of version 100 runs
    let fixed = codec
        .parse(&NBT, &with_version(99))
        .into_result()
        .expect("decode");
    assert_eq!(count_marked(&fixed.value), 3);
    assert!(fixed.value.map_get("DataVersion").is_none());
    // stored version 100: nothing to fix, the tag is still stripped
    let current = codec
        .parse(&NBT, &with_version(100))
        .into_result()
        .expect("decode");
    assert_eq!(count_marked(&current.value), 0);
    assert!(current.value.map_get("DataVersion").is_none());
    // no DataVersion entry: the default version applies
    let default = codec.parse(&NBT, &chunk()).into_result().expect("decode");
    assert_eq!(count_marked(&default.value), 3);
}

#[test]
fn update_tag_on_a_compound() {
    let fixer = builder_with_fix(true);
    let Ok(Tag::Compound(tag)) = chunk().to_tag() else {
        panic!("chunk is a compound")
    };
    let out = DataFixTypes::CHUNK.update_tag(&fixer, &tag, 99, 100);
    assert_eq!(count_marked(&Dyn::from(&Tag::Compound(out))), 3);
    let same = DataFixTypes::CHUNK.update_tag(&fixer, &tag, 100, 100);
    assert_eq!(same, tag);
}

#[test]
fn update_with_nothing_to_do_returns_the_input() {
    let fixer = builder_with_fix(true);
    for (from, to) in [(100, 100), (101, 100), (5023, 99)] {
        let out = DataFixTypes::CHUNK.update(&fixer, nbt_chunk(), from, to);
        assert_eq!(out.value, chunk(), "{from} -> {to}");
    }
}

#[test]
fn tagged_choice_fix_touches_one_choice() {
    let mut b = DataFixerBuilder::new(5023);
    b.add_schema(99, |k, p| Schema::new(k, p, Root));
    let v100 = b.add_schema(100, Schema::same);
    b.add_fixer(DataFix::new("ChestLockFix", &v100, false, |fix| {
        let schema = fix.input_schema();
        let ty = schema.get_type(&References::BLOCK_ENTITY);
        let chest_f = DSL::named_choice(
            "Chest",
            &schema.get_choice_type(&References::BLOCK_ENTITY, "Chest"),
        );
        fix.fix_type_everywhere_typed("ChestLock", &ty, move |typed| {
            typed.update_typed(&chest_f, |chest| {
                chest.update_remainder(|mut d: Dynamic| {
                    if let Dyn::Map(m) = &mut d.value {
                        Dyn::map_put(m, s("locked"), Dyn::Byte(1));
                    }
                    d
                })
            })
        })
    }));
    let out = b.build().update(&References::CHUNK, nbt_chunk(), 99, 100);
    let level = out.value.map_get("Level").expect("Level");
    let chest = match level.map_get("TileEntities") {
        Some(Dyn::List(l)) => &l[0],
        other => panic!("TileEntities: {other:?}"),
    };
    assert_eq!(chest.map_get("locked"), Some(&Dyn::Byte(1)));
    // only the chest changed
    let before = chunk();
    let before_level = before.map_get("Level").unwrap();
    assert_eq!(level.map_get("xPos"), Some(&Dyn::Int(3)));
    assert!(
        level
            .map_get("Entities")
            .unwrap()
            .java_eq(before_level.map_get("Entities").unwrap(), false)
    );
    let before_chest = match before_level.map_get("TileEntities") {
        Some(Dyn::List(l)) => &l[0],
        _ => unreachable!(),
    };
    assert!(
        chest
            .map_get("Items")
            .unwrap()
            .java_eq(before_chest.map_get("Items").unwrap(), false)
    );
}

#[test]
fn hook_and_compound_list_round_trip() {
    let schema = Schema::new(990, None, Root);
    let ty = schema.get_type(&References::HOTBAR);
    let input = Dynamic::new(
        NBT,
        Dyn::map_of([
            ("a", Dyn::Int(1)),
            ("junk", Dyn::Int(3)),
            ("b", Dyn::Int(2)),
        ]),
    );
    let (typed, _) = ty.read_typed(&input).into_result().expect("decode");
    let out = typed.write().into_result().expect("encode");
    // the pre-read hook dropped `junk`, the post-write hook added `stamp`
    let expected = Dyn::map_of([
        ("a", Dyn::Int(1)),
        ("b", Dyn::Int(2)),
        ("stamp", Dyn::Int(9)),
    ]);
    assert!(out.value.java_eq(&expected, false), "{:?}", out.value);
}

#[test]
fn update_error_returns_the_input_and_logs() {
    let fixer = builder_with_fix(true);
    // a chunk whose root is not a compound cannot be decoded
    let input = Dynamic::new(NBT, Dyn::Int(5));
    let (out, logged) = capture_logs(|| fixer.update(&References::CHUNK, input, 99, 100));
    assert_eq!(out.value, Dyn::Int(5));
    assert!(
        logged.iter().any(|l| l.starts_with("ERROR: ")),
        "no error logged: {logged:?}"
    );
}
