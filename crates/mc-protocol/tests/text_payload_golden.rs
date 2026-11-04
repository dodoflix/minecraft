//! `show_item` hover components through the typed `DataComponentPatch` codec (the hooks `mc_protocol` installs into
//! `mc_text`) against vanilla's `ComponentSerialization.CODEC`. Corpus: `fixtures/components/text/corpus.txt`;
//! regenerate golden: `scripts/vanilla-java.sh crates/mc-text/tests/fixtures/Golden.java
//! crates/mc-protocol/tests/fixtures/components/text/corpus.txt > crates/mc-protocol/tests/fixtures/components/text/golden.jsonl`.

use mc_nbt::{nbt_io, NbtAccounter, Tag};
use mc_text::{set_validators, Component, Validators};
use serde_json::Value;

const GOLDEN: &str = include_str!("fixtures/components/text/golden.jsonl");

fn item_exists(id: &mc_core::ident::Identifier) -> bool {
    mc_data::Item::by_name(&id.to_string()).is_some()
}

fn tag_from_hex(hex: &str) -> Tag {
    let bytes: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
    nbt_io::read_unnamed_tag(&mut bytes.as_slice(), &mut NbtAccounter::unlimited_heap()).expect("fixture nbt")
}

fn same_key_order(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|((ka, va), (kb, vb))| ka == kb && same_key_order(va, vb))
        }
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same_key_order(p, q)),
        _ => true,
    }
}

#[test]
fn hover_item_components_match_vanilla() {
    set_validators(Validators { item_exists, ..Validators::default() });
    mc_protocol::components::install_text_hooks();
    let (mut ok_cases, mut err_cases, mut failures) = (0, 0, Vec::new());
    for line in GOLDEN.lines() {
        let rec: Value = serde_json::from_str(line).unwrap();
        let src = rec["src"].as_str().unwrap();
        let input = rec["input"].as_str().unwrap();
        let decoded = if rec["kind"] == "J" {
            Component::from_json(&serde_json::from_str::<Value>(input).unwrap())
        } else {
            Component::from_nbt(&tag_from_hex(input))
        };
        let expect_ok = rec["ok"].as_bool().unwrap();
        let c = match (decoded, expect_ok) {
            (Err(e), true) => {
                failures.push(format!("{src}\n   vanilla accepted, we rejected: {e}"));
                continue;
            }
            (Ok(c), false) => {
                failures.push(format!("{src}\n   vanilla rejected ({}), we accepted: {c}", rec["err"].as_str().unwrap()));
                continue;
            }
            (Err(_), false) => {
                err_cases += 1;
                continue;
            }
            (Ok(c), true) => c,
        };
        ok_cases += 1;
        if let Some(expected) = rec.get("json") {
            let expected: Value = serde_json::from_str(expected.as_str().unwrap()).unwrap();
            match c.to_json() {
                Ok(got) if got == expected && same_key_order(&got, &expected) => {}
                other => failures.push(format!("{src}\n   json expected {expected}\n   got      {other:?}")),
            }
        }
        if let Some(expected) = rec.get("nbt") {
            let expected = tag_from_hex(expected.as_str().unwrap());
            match c.to_nbt() {
                Ok(got) if got == expected => {}
                other => failures.push(format!("{src}\n   nbt expected {expected}\n   got      {other:?}")),
            }
        }
    }
    assert!(failures.is_empty(), "{} failure(s) ({ok_cases} ok / {err_cases} rejected passed):\n{}", failures.len(), failures.join("\n"));
}
