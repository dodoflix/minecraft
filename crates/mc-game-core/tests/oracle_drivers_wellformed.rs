//! Cheap guard for the Java oracle drivers: a duplicate `case "cmd" ->` label in `EntityOracleDriver.java` (two helpers adding the same
//! command) makes `scripts/entity-oracle.sh` fail to compile for EVERY scenario. This test needs no JVM; `scripts/check-oracle-java.sh` is the
//! full `javac` check.
extern crate mc_game_core as mc_game;
use std::collections::BTreeMap;

fn duplicate_cases(source: &str) -> Vec<String> {
    // the command switch is `switch (cmd) {` ... first `default ->` after it; dump kinds are `switch (kind)` and checked separately
    let mut dups = Vec::new();
    for selector in ["switch (cmd) {", "switch (kind) {", "switch (what) {"] {
        let Some(start) = source.find(selector) else { continue };
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        let mut depth = 0i32;
        let mut line_no = source[..start].matches('\n').count() + 1;
        for line in source[start..].lines() {
            let t = line.trim_start();
            // labels sit directly inside the switch body (depth 1)
            if depth == 1 {
                if let Some(rest) = t.strip_prefix("case ") {
                    for label in rest.split("->").next().unwrap_or("").split(',') {
                        let label = label.trim().to_string();
                        if label.starts_with('"') {
                            if let Some(first) = seen.insert(label.clone(), line_no) {
                                dups.push(format!("{selector} duplicate {label} at line {line_no} (first at {first})"));
                            }
                        }
                    }
                }
            }
            for c in line.chars() {
                match c {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            if depth <= 0 {
                break;
            }
            line_no += 1;
        }
    }
    dups
}

#[test]
fn entity_oracle_driver_has_no_duplicate_command_labels() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../mc-game/tests/oracle/EntityOracleDriver.java");
    let source = std::fs::read_to_string(path).unwrap();
    let dups = duplicate_cases(&source);
    assert!(dups.is_empty(), "EntityOracleDriver.java: {dups:#?}");
}

#[test]
fn duplicate_detector_detects() {
    let src = "switch (cmd) {\n   case \"a\" -> {\n      switch (x) {\n         case \"a\" -> 1;\n      }\n   }\n   case \"b\", \"c\" -> 2;\n   case \"c\" -> 3;\n}\n";
    assert_eq!(duplicate_cases(src).len(), 1);
}
