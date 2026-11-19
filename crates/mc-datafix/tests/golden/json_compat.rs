//! The Gson-compatible JSON stack (`mc_core::gson`) and `LegacyComponentDataFixUtils` against the real Gson and vanilla
//! classes (`fixtures/json_compat/JsonCompatGolden.java`, regenerate with `fixtures/json_compat/gen.sh`).

use mc_core::gson;
use mc_datafix::legacy_component_data_fix_utils as legacy;
use mc_dfu::{Dyn, Dynamic, NBT};

const GOLDEN: &str = include_str!("../fixtures/json_compat.tsv");

/// Inverse of the recorder's `esc`.
fn unesc(s: &str) -> String {
    let mut units: Vec<u16> = Vec::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            units.extend(c.encode_utf16(&mut [0; 2]).iter());
            continue;
        }
        let unit = match chars.next().unwrap() {
            '\\' => u16::from(b'\\'),
            't' => u16::from(b'\t'),
            'n' => u16::from(b'\n'),
            'r' => u16::from(b'\r'),
            'u' => {
                let hex: String = chars.by_ref().take(4).collect();
                u16::from_str_radix(&hex, 16).unwrap()
            }
            other => panic!("bad escape \\{other}"),
        };
        units.push(unit);
    }
    String::from_utf16(&units).expect("the recording has no lone surrogates")
}

fn err(e: gson::JsonError) -> String {
    format!("ERR:{}", e.0)
}

fn run(op: &str, input: &str) -> String {
    match op {
        "lenient_stable" => {
            gson::parse_lenient_json_parser(input).map_or_else(err, |e| e.to_stable_string())
        }
        "strict_stable" => {
            gson::parse_strict_json_parser(input).map_or_else(err, |e| e.to_stable_string())
        }
        "rewrite_from_lenient" => legacy::rewrite_from_lenient(input),
        "strict_valid" => {
            legacy::is_strictly_valid_json(&Dynamic::new(NBT, Dyn::Str(input.into()))).to_string()
        }
        "extract_translation" => {
            legacy::extract_translation_string(input).map_or("none".into(), |v| format!("some:{v}"))
        }
        "text_json" => legacy::create_text_component_json(input),
        "translatable_json" => legacy::create_translatable_component_json(input),
        "empty_component" => legacy::create_empty_component(NBT).as_string_or("?"),
        "plain_component" => legacy::create_plain_text_component(NBT, input).as_string_or("?"),
        "translatable_component" => {
            legacy::create_translatable_component(NBT, input).as_string_or("?")
        }
        other => panic!("unknown op {other}"),
    }
}

/// `ERR:<exception class>:<message>` -> `ERR:<message>` (the Rust side has no exception classes).
fn without_class(want: String) -> String {
    match want.strip_prefix("ERR:") {
        Some(rest) => format!("ERR:{}", rest.split_once(':').unwrap().1),
        None => want,
    }
}

#[test]
fn json_stack_matches_gson() {
    let (mut bad, mut n) = (Vec::new(), 0);
    for line in GOLDEN.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let (op, input, want) = (f[0], unesc(f[1]), without_class(unesc(f[2])));
        let got = run(op, &input);
        if got != want {
            bad.push(format!(
                "{op} {input:?}\n   java: {want:?}\n   rust: {got:?}"
            ));
        }
        n += 1;
    }
    assert!(n > 1700, "{n} cases");
    assert!(
        bad.is_empty(),
        "{} of {n} cases differ; first:\n{}",
        bad.len(),
        bad.iter().take(25).cloned().collect::<Vec<_>>().join("\n")
    );
}
