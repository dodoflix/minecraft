//! Cost of the first `CHUNK` update from the oldest to the current version, which builds and applies the rules of the
//! whole chain without vanilla's point-free optimizer. Prints the timings and the peak resident memory of the
//! process; run it alone (`cargo test -p mc-datafix --test golden rule_cost -- --ignored --nocapture`), the caches
//! are process wide.
//!
//! The chain registers every fix `data_fixer()` has, and one stand-in per recorded data fix (`fixtures/fixes.tsv`) it
//! does not have yet: an identity rewrite of the type named by `MC_RULE_COST_TARGET` (`item_stack` (default),
//! `entity_tree`, `block_entity` or `chunk`) in the fix's schema, which also changes the type when the schema's type
//! differs from its parent's.

use crate::recorded_chain::parse;
use mc_datafix::{DataFix, DataFixerBuilder, References, TypeReference, data_fixer};
use mc_dfu::{Dyn, Dynamic, NBT};
use std::time::Instant;

fn status_kb(field: &str) -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").expect("/proc/self/status");
    let line = status.lines().find(|l| l.starts_with(field)).expect(field);
    line.split_whitespace()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .expect(field)
}

fn target() -> TypeReference {
    let name = std::env::var("MC_RULE_COST_TARGET").unwrap_or_else(|_| "item_stack".to_string());
    *References::ALL
        .iter()
        .find(|r| r.type_name() == name)
        .unwrap_or_else(|| panic!("no type {name}"))
}

#[test]
#[ignore = "measurement: prints timings and memory, run alone"]
fn chunk_rule_cost() {
    let started = Instant::now();
    let schemas = data_fixer();
    eprintln!(
        "data_fixer(): {} ms, peak {} MB",
        started.elapsed().as_millis(),
        status_kb("VmHWM:") / 1024
    );
    let target = target();
    let mut builder = DataFixerBuilder::new(mc_datafix::CURRENT_VERSION);
    for key in schemas.schema_keys() {
        builder.add_schema_object(schemas.get_schema(key).clone());
    }
    let rows = parse(include_str!("../fixtures/fixes.tsv"));
    let mut real = schemas.fixes().iter().peekable();
    for row in rows.iter().filter(|r| r.kind == "data") {
        let key = row.version * 10 + row.sub;
        if let Some(fix) = real.next_if(|f| f.version_key() == key && f.fix_name() == row.class) {
            builder.add_fixer(fix.clone());
            continue;
        }
        let schema = schemas.get_schema(key).clone();
        let changes_type = schema
            .parent()
            .is_some_and(|p| p.get_type(&target) != schema.get_type(&target));
        builder.add_fixer(DataFix::new(
            &row.class,
            &schema,
            changes_type,
            move |fix| {
                let ty = fix.input_schema().get_type(&target);
                if changes_type {
                    fix.write_and_read("measure", &ty, &fix.output_schema().get_type(&target))
                } else {
                    fix.fix_type_everywhere_typed("measure", &ty, |typed| typed)
                }
            },
        ));
    }
    let fixer = builder.build();
    let chunk = || Dynamic::new(NBT, Dyn::map_of([("Level", Dyn::map_of([]))]));
    for (from, to) in [(4899, 5023), (99, 5023)] {
        let started = Instant::now();
        fixer.update(&References::CHUNK, chunk(), from, to);
        eprintln!(
            "CHUNK {from} -> {to}: {} ms, peak {} MB",
            started.elapsed().as_millis(),
            status_kb("VmHWM:") / 1024
        );
    }
}
