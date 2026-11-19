//! The type of every `(schema version, Reference)` against the recording of the real `DataFixerUpper`
//! (`fixtures/schema_types.tsv.gz`): the sha256 of the normalised description must be equal. The schema versions we
//! do not register yet are exactly the lines of `fixtures/schema_types_expected_missing.txt` (delete the keys of a
//! ported schema in its commit; a listed key we now register fails the test, so the list only shrinks).

use crate::recorded_schemas::{
    Recorded, check_schema, description, first_difference, hash_description,
};
use mc_datafix::{DSL, References, Schema, SchemaDef, TemplateMap, data_fixer};
use std::collections::BTreeSet;
use std::sync::LazyLock;

static RECORDED: LazyLock<Recorded> =
    LazyLock::new(|| Recorded::parse(&mc_testkit::include_gz!("../fixtures/schema_types.tsv.gz")));
const EXPECTED_MISSING: &str = include_str!("../fixtures/schema_types_expected_missing.txt");

#[test]
fn registered_schemas_match_the_recording() {
    let fixer = data_fixer();
    let registered: BTreeSet<i32> = fixer.schema_keys().collect();
    let mut differences = Vec::new();
    for key in &registered {
        differences.extend(check_schema(&RECORDED, fixer.get_schema(*key)));
    }
    assert!(
        differences.is_empty(),
        "{}",
        differences
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
    let expected: BTreeSet<i32> = EXPECTED_MISSING
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.parse().expect("schema key"))
        .collect();
    let missing: BTreeSet<i32> = RECORDED.keys().difference(&registered).copied().collect();
    let now_registered: Vec<_> = expected.difference(&missing).collect();
    let unlisted: Vec<_> = missing.difference(&expected).collect();
    assert!(
        now_registered.is_empty(),
        "registered now, remove from schema_types_expected_missing.txt: {now_registered:?}"
    );
    assert!(
        unlisted.is_empty(),
        "not registered and not listed in schema_types_expected_missing.txt: {unlisted:?}"
    );
    let recorded_keys = RECORDED.keys();
    let unknown: Vec<_> = registered.difference(&recorded_keys).collect();
    assert!(
        unknown.is_empty(),
        "no such schema key in vanilla: {unknown:?}"
    );
}

#[test]
fn recording_covers_vanillas_312_schema_keys() {
    let keys = RECORDED.keys();
    assert_eq!(keys.len(), 312);
    assert_eq!(keys.first(), Some(&990));
    assert!(keys.contains(&49970));
    let types = RECORDED.types(990).expect("schema 99");
    assert!(types.contains_key("item_stack") && types.contains_key("chunk"));
}

/// A schema with an ITEM_STACK (recursive) and an ITEM_NAME; `with_count` adds a field.
struct Mini {
    with_count: bool,
}

impl SchemaDef for Mini {
    fn register_entities(&self, _this: &Schema, _schema: &Schema) -> TemplateMap {
        TemplateMap::new()
    }
    fn register_block_entities(&self, _this: &Schema, _schema: &Schema) -> TemplateMap {
        TemplateMap::new()
    }
    fn register_types(&self, _this: &Schema, schema: &Schema, _e: &TemplateMap, _b: &TemplateMap) {
        schema.register_type(false, &References::ITEM_NAME, |_| {
            DSL::const_type(&DSL::namespaced_string())
        });
        let with_count = self.with_count;
        schema.register_type(true, &References::ITEM_STACK, move |s| {
            if with_count {
                DSL::optional_fields(&[
                    ("id", References::ITEM_NAME.in_(s)),
                    ("Count", DSL::const_type(&DSL::int_type())),
                ])
            } else {
                DSL::optional_fields(&[("id", References::ITEM_NAME.in_(s))])
            }
        });
    }
}

#[test]
fn a_wrong_schema_is_reported_with_its_version_and_type() {
    let right = Schema::new(990, None, Mini { with_count: false });
    let wrong = Schema::new(990, None, Mini { with_count: true });
    let mut text = String::from("990\t*\t-\n");
    for ty in ["item_name", "item_stack"] {
        let d = description(&right, ty).expect("a References type");
        text.push_str(&format!("990\t{ty}\t{}\n", hash_description(&d)));
    }
    let recorded = Recorded::parse(&text);
    assert!(check_schema(&recorded, &right).is_empty());
    let differences = check_schema(&recorded, &wrong);
    assert_eq!(differences.len(), 1, "{differences:?}");
    assert_eq!(
        (differences[0].key, differences[0].ty.as_str()),
        (990, "item_stack")
    );
    let node = first_difference(
        &description(&right, "item_stack").unwrap(),
        &description(&wrong, "item_stack").unwrap(),
    )
    .expect("the descriptions differ");
    assert!(node.starts_with("node "), "{node}");
}
