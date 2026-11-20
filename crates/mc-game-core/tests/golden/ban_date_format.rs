//! `BanListEntry.DATE_FORMAT` (`yyyy-MM-dd HH:mm:ss Z`, lenient) against `SimpleDateFormat` of JDK 25
//! (`fixtures/LegacyDateGolden.java`): parsing of valid, overflowing, odd and trailing-text inputs, the zone left by a
//! parsed zone name, and formatting in several zones (the Julian calendar before 1582 included).

use chrono::DateTime;
use mc_game::server::players::ban_date_format::DateFormat;
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

/// What our parse gives for the Java result: a date that `chrono` cannot hold is none.
fn expected(result: &str) -> Option<DateTime<chrono::Utc>> {
    (result != "null")
        .then(|| DateTime::from_timestamp_millis(result.parse().unwrap()))
        .flatten()
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
    // the default zone of the golden is Europe/Berlin: +01:00 at both instants
    const BERLIN: i32 = 3600;
    let mut n = 0;
    for line in GOLDEN.lines().filter_map(|l| l.strip_prefix("seq\t")) {
        let mut fields = line.split('\t');
        let inputs = fields.next().unwrap();
        let format = DateFormat::new();
        let at = |epoch: i64| {
            format.format_in_default_zone(&DateTime::from_timestamp_millis(epoch).unwrap(), BERLIN)
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
        // out of the range of `chrono`: not representable
        let Some(date) = DateTime::from_timestamp_millis(epoch) else {
            continue;
        };
        assert_eq!(
            DateFormat::new().format_in_default_zone(&date, offset),
            want,
            "{epoch} at {offset}"
        );
        n += 1;
    }
    assert!(n > 150, "{n}");
}
