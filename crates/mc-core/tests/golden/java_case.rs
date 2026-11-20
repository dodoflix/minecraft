//! `String.toLowerCase(Locale.ROOT)` against the real JDK 25 (`fixtures/java_case/CaseGolden.java`): every code point
//! that changes, and random strings of letters, sigmas and word-break characters.

use mc_core::java_case::to_lower_case_root;
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
