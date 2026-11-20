//! The context of a syntax error is a window of the input in UTF-16 units and can start or end inside a surrogate pair:
//! `fixtures/lone_surrogate.tsv`, recorded by `crates/mc-protocol/tests/fixtures/lone_surrogate/LoneSurrogateGolden.java`.
//! The window is a Java string; the recorded context and message are compared exactly.

use mc_brigadier::message::literal as msg;
use mc_brigadier::{SimpleCommandExceptionType, StringReader};
use mc_core::java_string::JavaString;
use mc_testkit::unesc_units;

const GOLDEN: &str = include_str!("../fixtures/lone_surrogate.tsv");

fn java(cell: &str) -> JavaString {
    JavaString::from_utf16_units(&unesc_units(cell))
}

#[test]
fn error_contexts_match_vanilla() {
    let mut n = 0;
    for line in GOLDEN.lines() {
        let c: Vec<&str> = line.split('\t').collect();
        let cursor: usize = c[0].strip_prefix("ctx:").unwrap().parse().unwrap();
        // the one input that holds a lone surrogate cannot be a Rust string; the network never delivers one
        let Ok(input) = String::from_utf16(&unesc_units(c[1])) else {
            continue;
        };
        let mut reader = StringReader::new(input);
        reader.set_cursor(cursor);
        let e =
            SimpleCommandExceptionType::new(msg("Expected something")).create_with_context(&reader);
        assert_eq!(e.get_context().unwrap(), java(c[2]), "{line}");
        assert_eq!(e.get_message(), java(c[3]), "{line}");
        n += 1;
    }
    assert_eq!(n, 9, "rows of valid input");
}
