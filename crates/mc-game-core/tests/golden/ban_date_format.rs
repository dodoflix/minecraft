//! `BanListEntry.DATE_FORMAT` (`yyyy-MM-dd HH:mm:ss Z`, lenient) against `SimpleDateFormat` of JDK 25
//! (`fixtures/LegacyDateGolden.java`): parsing of valid, overflowing, odd and trailing-text inputs, the zone left by a
//! parsed zone name, and formatting in several zones (the Julian calendar before 1582 included).

use mc_game::server::players::ban_date_format::{DateFormat, DefaultOffset};
use mc_game::server::players::ban_list_entry::Date;
use std::sync::LazyLock;

static GOLDEN: LazyLock<String> = LazyLock::new(|| {
    mc_testkit::include_gz!("../../../mc-game/tests/fixtures/legacy_date_golden.txt.gz")
});

fn text(hex: &str) -> String {
    let units: Vec<u16> = hex
        .split(',')
        .filter(|u| !u.is_empty())
        .map(|u| u16::from_str_radix(u, 16).unwrap())
        .collect();
    String::from_utf16(&units).unwrap()
}

/// What our parse gives for the Java result.
fn expected(result: &str) -> Option<Date> {
    (result != "null").then(|| Date::from_millis(result.parse().unwrap()))
}

#[test]
fn parse_matches_java() {
    let mut failures = Vec::new();
    let mut n = 0;
    for line in GOLDEN.lines().filter_map(|l| l.strip_prefix("parse\t")) {
        let (input, result) = line.split_once('\t').unwrap();
        let input = text(input);
        let got = DateFormat::new().parse_in_default_zone(&input, Some("UTC"));
        if got != expected(result) {
            failures.push(format!("{input:?}: want {result}, got {got:?}"));
        }
        n += 1;
    }
    assert!(n > 20000, "{n}");
    assert!(
        failures.is_empty(),
        "{} of {n} differ:\n{}",
        failures.len(),
        failures[..failures.len().min(40)].join("\n")
    );
}

#[test]
fn zone_of_the_shared_format_matches_java() {
    let mut n = 0;
    for line in GOLDEN.lines().filter_map(|l| l.strip_prefix("seq\t")) {
        let mut fields = line.split('\t');
        let inputs = fields.next().unwrap();
        let format = DateFormat::new();
        let at = |epoch: i64| {
            format.format_in_default_zone(
                &Date::from_millis(epoch),
                DefaultOffset::Zone("Europe/Berlin"),
            )
        };
        let expected_start = fields.next().unwrap();
        assert_eq!(
            format!("{}|{}", at(0), at(1_700_000_000_000)),
            expected_start
        );
        for (input, want) in inputs.split(';').zip(fields) {
            let input = text(input);
            let parsed = format.parse_in_default_zone(&input, Some("Europe/Berlin"));
            let (millis, rest) = want.split_once('|').unwrap();
            assert_eq!(parsed, expected(millis), "{input:?}");
            assert_eq!(
                format!("{}|{}", at(0), at(1_700_000_000_000)),
                rest,
                "after {input:?}"
            );
            n += 1;
        }
    }
    assert!(n >= 14, "{n}");
}

#[test]
fn format_matches_java() {
    let mut n = 0;
    for line in GOLDEN.lines().filter_map(|l| l.strip_prefix("fmt\t")) {
        let mut fields = line.splitn(3, '\t');
        let epoch: i64 = fields.next().unwrap().parse().unwrap();
        let offset: i32 = fields.next().unwrap().parse().unwrap();
        let want = fields.next().unwrap();
        assert_eq!(
            DateFormat::new()
                .format_in_default_zone(&Date::from_millis(epoch), DefaultOffset::Fixed(offset)),
            want,
            "{epoch} at {offset}"
        );
        n += 1;
    }
    assert!(n > 150, "{n}");
}

#[test]
fn ends_of_the_long_range_in_a_named_zone_match_java() {
    const INSTANTS: [i64; 6] = [
        i64::MAX,
        i64::MIN,
        i64::MAX - 86_400_000 * 120,
        i64::MAX - 86_400_000 * 300,
        i64::MIN + 86_400_000 * 120,
        8_210_266_876_800_000,
    ];
    let mut n = 0;
    for line in GOLDEN.lines().filter_map(|l| l.strip_prefix("zfmt\t")) {
        let mut fields = line.split('\t');
        let zone = text(fields.next().unwrap());
        let format = DateFormat::new();
        format
            .parse_in_default_zone(&format!("2013-02-03 04:05:06 {zone}"), Some("UTC"))
            .unwrap();
        for (epoch, want) in INSTANTS.iter().zip(fields) {
            assert_eq!(
                format
                    .format_in_default_zone(&Date::from_millis(*epoch), DefaultOffset::Zone("UTC")),
                want,
                "{epoch} in {zone}"
            );
            n += 1;
        }
    }
    assert!(n >= 24, "{n}");
}

#[test]
fn rule_cache_of_the_zone_matches_java() {
    let mut n = 0;
    for line in GOLDEN.lines().filter_map(|l| l.strip_prefix("zseq\t")) {
        let mut fields = line.split('\t');
        let format = DateFormat::new();
        for (op, want) in fields.next().unwrap().split(';').zip(fields) {
            let got = match op.split_at(1) {
                ("p", input) => {
                    // the default zone of these golden lines is UTC
                    let parsed = format.parse_in_default_zone(&text(input), Some("UTC"));
                    parsed.map_or("null".to_owned(), |d| d.millis().to_string())
                }
                (_, epoch) => format.format_in_default_zone(
                    &Date::from_millis(epoch.parse().unwrap()),
                    DefaultOffset::Zone("UTC"),
                ),
            };
            assert_eq!(got, want, "{line}: {op}");
            n += 1;
        }
    }
    assert!(n >= 100, "{n}");
}

#[test]
fn rule_cache_of_the_default_zone_matches_java() {
    let mut n = 0;
    for line in GOLDEN.lines().filter_map(|l| l.strip_prefix("dseq\t")) {
        let mut fields = line.split('\t');
        let zone = fields.next().unwrap();
        let format = DateFormat::new();
        for (op, want) in fields.next().unwrap().split(';').zip(fields) {
            let got = match op.split_at(1) {
                ("p", input) => {
                    let parsed = format.parse_in_default_zone(&text(input), Some(zone));
                    parsed.map_or("null".to_owned(), |d| d.millis().to_string())
                }
                (_, epoch) => format.format_in_default_zone(
                    &Date::from_millis(epoch.parse().unwrap()),
                    DefaultOffset::Zone(zone),
                ),
            };
            assert_eq!(got, want, "{line}: {op}");
            n += 1;
        }
    }
    assert!(n >= 80, "{n}");
}
