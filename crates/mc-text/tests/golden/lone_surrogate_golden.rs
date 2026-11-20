//! Components made from strings with lone UTF-16 surrogates against vanilla: `fixtures/lone_surrogate.tsv`, recorded by
//! `crates/mc-protocol/tests/fixtures/lone_surrogate/LoneSurrogateGolden.java`.
//!
//! A Rust `String` cannot hold a lone surrogate, so each recorded one is compared as U+FFFD (in text) or as
//! `ef bf bd` (in modified UTF-8). Rows that need a lone surrogate in the input are skipped.

use mc_nbt::snbt::parse;
use mc_nbt::{Tag, nbt_io};
use mc_text::Component;
use mc_text::nbt_text::{Styling, TextComponentTagVisitor};

const GOLDEN: &str = include_str!("../fixtures/lone_surrogate.tsv");

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(h: &str) -> Vec<u8> {
    (0..h.len() / 2)
        .map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

/// Inverse of the recorder's `esc`, as UTF-16 units.
fn unesc_units(s: &str) -> Vec<u16> {
    let mut units: Vec<u16> = Vec::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            units.extend(c.encode_utf16(&mut [0; 2]).iter());
            continue;
        }
        units.push(match chars.next().unwrap() {
            '\\' => u16::from(b'\\'),
            't' => u16::from(b'\t'),
            'n' => u16::from(b'\n'),
            'r' => u16::from(b'\r'),
            'u' => u16::from_str_radix(&chars.by_ref().take(4).collect::<String>(), 16).unwrap(),
            other => panic!("bad escape \\{other}"),
        });
    }
    units
}

fn lossy(s: &str) -> String {
    String::from_utf16_lossy(&unesc_units(s))
}

/// Modified UTF-8 bytes with each lone surrogate (`ed a0..bf xx` not forming a pair) replaced by U+FFFD.
fn lossy_hex(h: &str) -> String {
    let b = unhex(h);
    let triple = |i: usize| b.len() >= i + 3 && b[i] == 0xED && b[i + 1] >= 0xA0;
    let (mut out, mut i) = (Vec::new(), 0);
    while i < b.len() {
        if triple(i) {
            if b[i + 1] < 0xB0 && triple(i + 3) && b[i + 4] >= 0xB0 {
                out.extend_from_slice(&b[i..i + 6]);
                i += 6;
            } else {
                out.extend_from_slice(&[0xEF, 0xBF, 0xBD]);
                i += 3;
            }
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    hex(&out)
}

fn nbt_hex(c: &Component) -> String {
    let mut out = Vec::new();
    nbt_io::write_any_tag(&c.to_nbt().unwrap(), &mut out).unwrap();
    hex(&out)
}

fn rows(op: &str) -> impl Iterator<Item = Vec<&'static str>> + '_ {
    GOLDEN
        .lines()
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .filter(move |c| c[0] == op)
}

#[test]
fn components_from_snbt_match_vanilla() {
    let mut n = 0;
    for c in rows("comp_snbt") {
        let input = String::from_utf16(&unesc_units(c[1])).unwrap();
        let component = Component::from_nbt(&parse(&input).unwrap()).unwrap();
        // `getString` joins the halves of a pair that sit in different components into one character
        if !input.contains("extra") && !input.starts_with('[') {
            assert_eq!(component.get_string(), lossy(c[2]), "getString of {input}");
        }
        assert_eq!(nbt_hex(&component), lossy_hex(c[4]), "nbt of {input}");
        n += 1;
    }
    assert_eq!(n, 10);
}

#[test]
fn nbt_shown_as_text_matches_vanilla() {
    let mut n = 0;
    for c in rows("tag_pretty") {
        let input = String::from_utf16(&unesc_units(c[1])).unwrap();
        // two keys that only differ by a lone surrogate collapse into one here
        if c[2].starts_with("ERR:") || input.contains("\\ufffd") {
            continue;
        }
        let tag: Tag = parse(&input).unwrap();
        let component =
            TextComponentTagVisitor::with_options("  ", Styling::Plain, true).visit(&tag);
        assert_eq!(component.get_string(), lossy(c[2]), "getString of {input}");
        assert_eq!(nbt_hex(&component), lossy_hex(c[4]), "nbt of {input}");
        n += 1;
    }
    assert!(n >= 10, "{n} rows");
}
