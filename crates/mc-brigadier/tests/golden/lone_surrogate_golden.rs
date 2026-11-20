//! The context of a syntax error is a window of the input in UTF-16 units and can start or end inside a surrogate pair:
//! `fixtures/lone_surrogate.tsv`, recorded by `crates/mc-protocol/tests/fixtures/lone_surrogate/LoneSurrogateGolden.java`.
//! A Rust `String` cannot hold the resulting lone surrogate, so each is compared as U+FFFD.

use mc_brigadier::message::literal as msg;
use mc_brigadier::{SimpleCommandExceptionType, StringReader};

const GOLDEN: &str = include_str!("../fixtures/lone_surrogate.tsv");

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
            'u' => u16::from_str_radix(&chars.by_ref().take(4).collect::<String>(), 16).unwrap(),
            other => panic!("bad escape \\{other}"),
        });
    }
    units
}

#[test]
fn error_contexts_match_vanilla() {
    let mut n = 0;
    for line in GOLDEN.lines() {
        let c: Vec<&str> = line.split('\t').collect();
        let cursor: usize = c[0].strip_prefix("ctx:").unwrap().parse().unwrap();
        let Ok(input) = String::from_utf16(&unesc_units(c[1])) else {
            continue;
        };
        let mut reader = StringReader::new(input);
        reader.set_cursor(cursor);
        let e =
            SimpleCommandExceptionType::new(msg("Expected something")).create_with_context(&reader);
        assert_eq!(
            e.get_context().unwrap(),
            String::from_utf16_lossy(&unesc_units(c[2])),
            "{line}"
        );
        assert_eq!(
            e.get_message(),
            String::from_utf16_lossy(&unesc_units(c[3])),
            "{line}"
        );
        n += 1;
    }
    assert!(n >= 9, "{n} rows");
}
