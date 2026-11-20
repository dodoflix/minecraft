//! The committed recordings (`fixtures/fixes.tsv`, `fixtures/schema_types.tsv.gz`) against the live vanilla classes, and
//! the type description grammar against the real DSL.

use crate::oracle_client::run_verb;
use crate::recorded_schemas::{Recorded, check_schema, description, first_difference};
use mc_datafix::{DSL, References, Schema, SchemaDef, TemplateMap, data_fixer};

const FIXES: &str = include_str!("../fixtures/fixes.tsv");

#[test]
#[ignore = "oracle: runs vanilla's DataFixers in a JVM"]
fn fixes_list_matches_the_recording() {
    let live = run_verb(&["fixes-list"]);
    let data = live.lines().filter(|l| l.contains("\tdata\t")).count();
    let file = live.lines().filter(|l| l.contains("\tfile\t")).count();
    assert_eq!((data, file), (424, 8), "registered fixes of vanilla");
    assert_eq!(
        live, FIXES,
        "regenerate with scripts/datafix-oracle.sh --regen"
    );
}

#[test]
#[ignore = "oracle: runs vanilla's DataFixers in a JVM"]
fn schema_dump_is_deterministic_and_matches_the_recording() {
    let first = run_verb(&["schema-dump", "all"]);
    let second = run_verb(&["schema-dump", "all"]);
    assert!(first == second, "two runs of schema-dump all differ");
    let recorded = mc_testkit::include_gz!("../fixtures/schema_types.tsv.gz");
    assert!(
        first == recorded,
        "regenerate with scripts/datafix-oracle.sh --regen"
    );
}

/// The schema of `GrammarSample.java` with the ported DSL.
struct GrammarSample;

impl SchemaDef for GrammarSample {
    fn register_entities(&self, _this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = TemplateMap::new();
        schema.register(&mut map, "Pig", |s, _| {
            DSL::optional_fields(&[
                ("Saddle", DSL::const_type(&DSL::bool())),
                ("Item", References::ITEM_STACK.in_(s)),
            ])
        });
        schema.register_simple(&mut map, "Cow");
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
        use mc_datafix::HookFunction;
        schema.register_type(false, &References::ITEM_NAME, |_| {
            DSL::const_type(&DSL::namespaced_string())
        });
        schema.register_type(true, &References::ITEM_STACK, |s| {
            DSL::optional_fields(&[
                ("id", References::ITEM_NAME.in_(s)),
                ("Count", DSL::const_type(&DSL::byte_type())),
                ("Damage", DSL::const_type(&DSL::short_type())),
                (
                    "Hooked",
                    DSL::hook(
                        &DSL::optional_fields(&[("A", DSL::const_type(&DSL::int_type()))]),
                        &HookFunction::identity(),
                        &HookFunction::identity(),
                    ),
                ),
            ])
        });
        let e = entity_types.clone();
        schema.register_type(true, &References::ENTITY, move |s| {
            DSL::tagged_choice_lazy("id", &DSL::string(), &e, s)
        });
        let b = block_entity_types.clone();
        schema.register_type(false, &References::BLOCK_ENTITY, move |s| {
            DSL::tagged_choice_lazy("id", &DSL::string(), &b, s)
        });
        schema.register_type(true, &References::ENTITY_TREE, |s| {
            DSL::optional_fields_with_rest(
                &[("Passengers", DSL::list(&References::ENTITY_TREE.in_(s)))],
                &References::ENTITY.in_(s),
            )
        });
        schema.register_type(false, &References::CHUNK, |s| {
            DSL::optional_fields(&[
                (
                    "Level",
                    DSL::optional_fields(&[
                        ("xPos", DSL::const_type(&DSL::long_type())),
                        ("yPos", DSL::const_type(&DSL::float_type())),
                        ("zPos", DSL::const_type(&DSL::double_type())),
                        ("Name", DSL::const_type(&DSL::string())),
                        (
                            "Palette",
                            DSL::compound_list(&References::ITEM_STACK.in_(s)),
                        ),
                    ]),
                ),
                (
                    "Extra",
                    DSL::optional_fields(&[
                        ("Flag", DSL::const_type(&DSL::bool())),
                        ("TileEntities", DSL::list(&References::BLOCK_ENTITY.in_(s))),
                    ]),
                ),
            ])
        });
    }
}

#[test]
#[ignore = "oracle: runs vanilla's DataFixers in a JVM"]
fn grammar_sample_matches_the_oracle() {
    let schema = Schema::new(990, None, GrammarSample);
    let mut names = schema.types();
    names.sort();
    let mut ours = String::new();
    for name in names {
        let reference = References::by_name(&name).expect("a References type");
        ours.push_str(&format!(
            "@@ {name}\n{}",
            schema.get_type(&reference).description()
        ));
    }
    let vanilla = run_verb(&["grammar-sample"]);
    if let Some(difference) = first_difference(&vanilla, &ours) {
        panic!("the descriptions differ at {difference}");
    }
}

/// Shows the first differing node for every registered type that differs from the recording (the committed golden only
/// has hashes). Passes without a JVM while no schema differs.
#[test]
#[ignore = "oracle: runs vanilla's DataFixers in a JVM"]
fn registered_schemas_match_live_descriptions() {
    let recorded = Recorded::parse(&mc_testkit::include_gz!("../fixtures/schema_types.tsv.gz"));
    let fixer = data_fixer();
    let mut report = Vec::new();
    for key in fixer.schema_keys() {
        let schema = fixer.get_schema(key);
        for difference in check_schema(&recorded, schema) {
            // a type only one side registers has no description on the other side to compare with
            let on_both = schema.types().contains(&difference.ty)
                && recorded
                    .types(key)
                    .is_some_and(|t| t.contains_key(&difference.ty));
            let node = on_both
                .then(|| description(schema, &difference.ty))
                .flatten()
                .and_then(|ours| {
                    let vanilla = run_verb(&["schema-dump", &key.to_string(), &difference.ty]);
                    first_difference(&vanilla, &ours)
                });
            report.push(format!("{difference}: {}", node.unwrap_or_default()));
        }
    }
    assert!(report.is_empty(), "{}", report.join("\n"));
}

/// The file fixer records of `crates/mc-world/tests/fixtures/filefix` (the minimal worlds and what earlier starts left
/// behind) against vanilla's `FileFixerUpper` now.
#[test]
#[ignore = "oracle: runs vanilla's FileFixerUpper in a JVM"]
fn file_fix_records_match_the_live_oracle() {
    let fixtures = crate::oracle_client::repo_root().join("crates/mc-world/tests/fixtures/filefix");
    let out = crate::oracle_client::repo_root().join("target/tmp/file-fix-oracle");
    std::fs::create_dir_all(&out).unwrap();
    let mut compared = 0;
    for (folder, saves) in [("worlds", false), ("interrupted", true)] {
        let mut worlds: Vec<_> = std::fs::read_dir(fixtures.join(folder))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.is_dir())
            .collect();
        worlds.sort();
        for world in worlds {
            let id = world.file_name().unwrap().to_str().unwrap().to_owned();
            let live = out.join(format!("{id}.txt"));
            let mut args = vec![
                "filefix",
                world.to_str().unwrap(),
                "--out",
                live.to_str().unwrap(),
            ];
            if saves {
                args.push("--saves");
            }
            run_verb(&args);
            let recorded_path = if saves {
                fixtures.join("interrupted").join(format!("{id}.txt"))
            } else {
                fixtures.join(format!("{id}.txt"))
            };
            assert_eq!(
                std::fs::read_to_string(&live).unwrap(),
                std::fs::read_to_string(&recorded_path).unwrap(),
                "{id}: regenerate with scripts/datafix-oracle.sh --regen"
            );
            compared += 1;
        }
    }
    assert!(compared >= 11, "{compared} records compared");
}
