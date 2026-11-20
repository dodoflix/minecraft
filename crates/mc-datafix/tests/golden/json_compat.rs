//! The Gson-compatible JSON stack (`mc_core::gson`) and `LegacyComponentDataFixUtils` against the real Gson and vanilla
//! classes (`fixtures/json_compat/JsonCompatGolden.java`, regenerate with `fixtures/json_compat/gen.sh`).

use mc_core::gson;
use mc_core::java_string::JavaString;
use mc_datafix::legacy_component_data_fix_utils as legacy;
use mc_dfu::{Dyn, Dynamic, NBT};
use mc_testkit::unesc_units;

const GOLDEN: &str = include_str!("../fixtures/json_compat.tsv");

/// An input: the recorded inputs hold no lone surrogates (JSON escapes are spelled out in ASCII).
fn unesc(s: &str) -> String {
    String::from_utf16(&unesc_units(s)).expect("the recorded inputs have no lone surrogates")
}

/// A recorded result, exactly (Java keeps lone surrogates).
fn unesc_result(s: &str) -> JavaString {
    JavaString::from_utf16_units(&unesc_units(s))
}

fn err(e: gson::JsonError) -> JavaString {
    format!("ERR:{}", e.0).into()
}

fn run(op: &str, input: &str) -> JavaString {
    let java = JavaString::from(input);
    match op {
        "lenient_stable" => {
            gson::parse_lenient_json_parser(input).map_or_else(err, |e| e.to_stable_string())
        }
        "strict_stable" => {
            gson::parse_strict_json_parser(input).map_or_else(err, |e| e.to_stable_string())
        }
        "rewrite_from_lenient" => legacy::rewrite_from_lenient(&java),
        "strict_valid" => legacy::is_strictly_valid_json(&Dynamic::new(NBT, Dyn::Str(java)))
            .to_string()
            .into(),
        "extract_translation" => {
            legacy::extract_translation_string(&java).map_or("none".into(), |v| {
                let mut out = JavaString::from("some:");
                out.push_java(&v);
                out
            })
        }
        "text_json" => legacy::create_text_component_json(input),
        "translatable_json" => legacy::create_translatable_component_json(input),
        "empty_component" => legacy::create_empty_component(NBT).as_string_or("?"),
        "plain_component" => legacy::create_plain_text_component(NBT, input).as_string_or("?"),
        "translatable_component" => {
            legacy::create_translatable_component(NBT, input).as_string_or("?")
        }
        "lazy_long" => gson::lazily_parsed_long_value(input)
            .map_or_else(|e| format!("ERR:{e}"), |v| v.to_string())
            .into(),
        "lazy_int" => gson::lazily_parsed_int_value(input)
            .map_or_else(|e| format!("ERR:{e}"), |v| v.to_string())
            .into(),
        other => panic!("unknown op {other}"),
    }
}

/// `ERR:<exception class>:<message>` -> `ERR:<message>` (the Rust side has no exception classes).
fn without_class(want: JavaString) -> JavaString {
    let text = want.as_lossy_str();
    let Some(rest) = text.strip_prefix("ERR:") else {
        return want;
    };
    let colon = 4 + rest.find(':').unwrap();
    let mut out = JavaString::from("ERR:");
    // the byte offset of the text counts a lone surrogate as three bytes; the units count it as one
    let from = text[..colon + 1].encode_utf16().count();
    out.push_java(&want.substring(from, want.utf16_len()));
    out
}

#[test]
fn json_stack_matches_gson() {
    let (mut bad, mut n) = (Vec::new(), 0);
    for line in GOLDEN.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let (op, input, want) = (f[0], unesc(f[1]), without_class(unesc_result(f[2])));
        let got = run(op, &input);
        if got != want {
            bad.push(format!(
                "{op} {input:?}\n   java: {want:?}\n   rust: {got:?}"
            ));
        }
        n += 1;
    }
    assert!(n > 1900, "{n} cases");
    assert!(
        bad.is_empty(),
        "{} of {n} cases differ; first:\n{}",
        bad.len(),
        bad.iter().take(25).cloned().collect::<Vec<_>>().join("\n")
    );
}
