//! Components made from strings with lone UTF-16 surrogates against vanilla: `fixtures/lone_surrogate.tsv`, recorded by
//! `crates/mc-protocol/tests/fixtures/lone_surrogate/LoneSurrogateGolden.java`. Every cell is replayed exactly:
//! `getString`, the stable JSON text, the NBT bytes (modified UTF-8 writes a lone surrogate as `ed a0 80`) and the
//! text of failures.

use mc_core::gson::parse_lenient_json_parser;
use mc_core::java_string::JavaString;
use mc_nbt::snbt::parse;
use mc_nbt::{Tag, nbt_io};
use mc_testkit::{hex, rows, unesc_units};
use mc_text::Component;
use mc_text::nbt_text::{Styling, TextComponentTagVisitor};

const GOLDEN: &str = include_str!("../fixtures/lone_surrogate.tsv");

/// A recorded string, exactly.
fn java(cell: &str) -> JavaString {
    JavaString::from_utf16_units(&unesc_units(cell))
}

/// A recorded input; the inputs spell a lone surrogate as the six characters `\ud800`, so they are valid text.
fn input(cell: &str) -> String {
    String::from_utf16(&unesc_units(cell)).unwrap()
}

/// The message of a recorded `ERR:<class>:<message>` cell.
fn err_message(cell: &str) -> JavaString {
    let cell = java(cell);
    let text = cell.as_lossy_str();
    let skip = text.match_indices(':').nth(1).expect("class and message").0 + 1;
    cell.substring(text[..skip].encode_utf16().count(), cell.utf16_len())
}

fn nbt_hex(c: &Component) -> String {
    let mut out = Vec::new();
    nbt_io::write_any_tag(&c.to_nbt().unwrap(), &mut out).unwrap();
    hex(&out)
}

/// The cells `getString`, stable JSON and NBT bytes of a component (`c[2..5]`).
fn check_component(component: &Component, c: &[&str], what: &str) {
    assert_eq!(
        component.get_java_string(),
        java(c[2]),
        "getString of {what}"
    );
    let json = component.to_json().unwrap().to_stable_string();
    assert_eq!(json, java(c[3]), "stable JSON of {what}");
    assert_eq!(nbt_hex(component), c[4], "nbt of {what}");
}

#[test]
fn components_from_json_match_vanilla() {
    let mut n = 0;
    for c in rows(GOLDEN, "comp_json") {
        let text = input(c[1]);
        let decoded = Component::from_json(&parse_lenient_json_parser(&text).unwrap());
        if c[2].starts_with("ERR:") {
            let e = decoded.expect_err(&text);
            assert_eq!(e.0, err_message(c[2]), "error of {text}");
        } else {
            check_component(&decoded.expect(&text), &c, &text);
        }
        n += 1;
    }
    assert_eq!(n, 13);
}

#[test]
fn components_from_snbt_match_vanilla() {
    let mut n = 0;
    for c in rows(GOLDEN, "comp_snbt") {
        let text = input(c[1]);
        let component = Component::from_nbt(&parse(&text).unwrap()).expect(&text);
        check_component(&component, &c, &text);
        n += 1;
    }
    assert_eq!(n, 10);
}

#[test]
fn nbt_shown_as_text_matches_vanilla() {
    let mut n = 0;
    for c in rows(GOLDEN, "tag_pretty") {
        let text = input(c[1]);
        if c[2].starts_with("ERR:") {
            let e = parse(&text).expect_err(&text);
            assert_eq!(
                e.message_with_context(),
                err_message(c[2]),
                "error of {text}"
            );
        } else {
            let tag: Tag = parse(&text).unwrap();
            let component =
                TextComponentTagVisitor::with_options("  ", Styling::Plain, true).visit(&tag);
            check_component(&component, &c, &text);
        }
        n += 1;
    }
    assert_eq!(n, 20);
}
