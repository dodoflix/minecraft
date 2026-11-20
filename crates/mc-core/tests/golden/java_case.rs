//! `String.toLowerCase(Locale.ROOT)` against the real JDK 25 (`fixtures/java_case/CaseGolden.java`): every code point
//! that changes, and random strings of letters, sigmas and word-break characters.

use mc_core::java_case::{self, to_lower_case_root};
use mc_core::java_string::JavaString;
use std::collections::HashMap;
use std::sync::LazyLock;

static GOLDEN: LazyLock<String> =
    LazyLock::new(|| mc_testkit::include_gz!("../fixtures/java_case/golden.txt.gz"));

fn units(hex: &str) -> Vec<u16> {
    hex.split(',')
        .map(|u| u16::from_str_radix(u, 16).unwrap())
        .collect()
}

fn java_string(hex: &str) -> JavaString {
    JavaString::from_utf16_units(&units(hex))
}

#[test]
fn lower_case_of_every_code_point_matches_java() {
    let changed: HashMap<u32, JavaString> = GOLDEN
        .lines()
        .filter_map(|l| l.strip_prefix("cp\t"))
        .map(|l| {
            let (cp, lower) = l.split_once('\t').unwrap();
            (u32::from_str_radix(cp, 16).unwrap(), java_string(lower))
        })
        .collect();
    assert!(changed.len() > 1400, "{}", changed.len());
    for cp in (0..=0x10FFFF).filter(|cp| !(0xD800..=0xDFFF).contains(cp)) {
        let c = char::from_u32(cp).unwrap();
        let s = c.to_string();
        let want = changed
            .get(&cp)
            .cloned()
            .unwrap_or_else(|| JavaString::from(s.as_str()));
        assert_eq!(to_lower_case_root(&s), want.as_lossy_str(), "U+{cp:04X}");
    }
}

#[test]
fn lower_case_of_strings_matches_java() {
    let mut n = 0;
    let mut failures = Vec::new();
    for line in GOLDEN.lines().filter_map(|l| l.strip_prefix("str\t")) {
        let (input, want) = line.split_once('\t').unwrap();
        let (input, want) = (java_string(input), java_string(want));
        let got = input.to_lower_case_root();
        if got != want {
            failures.push(format!("{input:?}: want {want:?}, got {got:?}"));
        }
        if let Some(text) = input.exact_str() {
            assert_eq!(to_lower_case_root(text), want.as_lossy_str(), "{input:?}");
        }
        n += 1;
    }
    assert!(n >= 40000, "{n}");
    assert!(
        failures.is_empty(),
        "{} of {n} differ:\n{}",
        failures.len(),
        failures[..failures.len().min(30)].join("\n")
    );
}

static EQUALS_IGNORE_CASE: LazyLock<String> =
    LazyLock::new(|| mc_testkit::include_gz!("../fixtures/java_case/equals_ignore_case.txt.gz"));

fn mapping(kind: &str) -> HashMap<u32, u32> {
    EQUALS_IGNORE_CASE
        .lines()
        .filter_map(|l| l.strip_prefix(kind)?.strip_prefix('\t'))
        .map(|l| {
            let (cp, to) = l.split_once('\t').unwrap();
            (
                u32::from_str_radix(cp, 16).unwrap(),
                u32::from_str_radix(to, 16).unwrap(),
            )
        })
        .collect()
}

#[test]
fn simple_case_mappings_of_every_code_point_match_java() {
    let (upper, lower) = (mapping("up"), mapping("lo"));
    assert!(upper.len() > 1400 && lower.len() > 1400);
    let mut wrong = Vec::new();
    for cp in (0..=0x10FFFF).filter(|cp| !(0xD800..=0xDFFF).contains(cp)) {
        let (up, lo) = (java_case::simple_upper(cp), java_case::simple_lower(cp));
        if up != upper.get(&cp).copied().unwrap_or(cp) {
            wrong.push(format!("toUpperCase U+{cp:04X}: got U+{up:04X}"));
        }
        if lo != lower.get(&cp).copied().unwrap_or(cp) {
            wrong.push(format!("toLowerCase U+{cp:04X}: got U+{lo:04X}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

fn from_hex_units(hex: &str) -> String {
    String::from_utf16(&units(hex)).unwrap()
}

#[test]
fn equals_ignore_case_matches_java() {
    let mut n = 0;
    for l in EQUALS_IGNORE_CASE.lines() {
        let fields: Vec<&str> = l.split('\t').collect();
        let (a, b, want) = match fields[..] {
            ["eq", a, b, want] => {
                let cp = |h| char::from_u32(u32::from_str_radix(h, 16).unwrap()).unwrap();
                (cp(a).to_string(), cp(b).to_string(), want)
            }
            ["str", a, b, want] => (from_hex_units(a), from_hex_units(b), want),
            _ => continue,
        };
        assert_eq!(
            java_case::equals_ignore_case(&a, &b),
            want == "1",
            "{a:?} {b:?}"
        );
        n += 1;
    }
    assert!(n > 40000, "{n}");
}
