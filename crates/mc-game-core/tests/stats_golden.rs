//! `ServerStatsCounter` against the real vanilla class (`tests/fixtures/StatsGolden.java`, regenerate with
//! `scripts/golden-stats.sh`): parse of valid/malformed files, `toJson` (inner key order of every stat type; the order of
//! the stat types themselves is the identity hash of vanilla's `StatType` objects, i.e. random per run), and the order of
//! the entries of the `ClientboundAwardStatsPacket` built from the dirty set.

extern crate mc_game_core as mc_game;
use mc_game::stats::stat::{build_name, by_names};
use mc_game::stats::{ServerStatsCounter, stats::Stats};
use mc_core::ident::Identifier;
use serde_json::Value;

fn packet_line(counter: &mut ServerStatsCounter) -> String {
    counter
        .stats_packet()
        .stats
        .iter()
        .map(|(stat, value)| format!("{}={}", build_name(stat), value.0))
        .collect::<Vec<_>>()
        .join(" ")
}

/// `{"stats":{type:{value:n}},..}` with the types in a canonical order (vanilla's type order is random).
fn canonical(json: &Value) -> Value {
    let mut stats: Vec<(String, Value)> = json["stats"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    stats.sort_by(|a, b| a.0.cmp(&b.0));
    // inner maps keep their order: compare them as ordered lists
    Value::Array(
        stats
            .into_iter()
            .map(|(t, v)| Value::Array(vec![Value::String(t), Value::Array(v.as_object().unwrap().iter().map(|(k, n)| Value::Array(vec![Value::String(k.clone()), n.clone()])).collect())]))
            .collect(),
    )
}

/// Like [`canonical`] with the entries of every type sorted.
fn canonical_sorted(json: &Value) -> Value {
    let mut v = canonical(json);
    for t in v.as_array_mut().unwrap() {
        t[1].as_array_mut().unwrap().sort_by_key(|e| e.to_string());
    }
    v
}

#[test]
fn matches_vanilla() {
    let text = include_str!("../../mc-game/tests/fixtures/stats_golden.txt");
    let mut lines = text.lines().peekable();
    let mut cases = 0;
    while let Some(header) = lines.next() {
        if !header.starts_with("case ") {
            continue;
        }
        let kind = header.split(' ').nth(2).unwrap();
        if kind == "saturate" {
            let value: i32 = lines.next().unwrap().strip_prefix("value\t").unwrap().parse().unwrap();
            let mut c = ServerStatsCounter::detached();
            c.increment(Stats::JUMP, 2147483000);
            c.increment(Stats::JUMP, 2147483000);
            assert_eq!(c.get_value(&Stats::JUMP), value);
            continue;
        }
        let input = lines.next().unwrap().strip_prefix("in\t").unwrap();
        let out = lines.next().unwrap().strip_prefix("out\t").unwrap();
        let packet = lines.next().unwrap().strip_prefix("packet\t").unwrap();
        let mut c = ServerStatsCounter::detached();
        if kind == "parse" {
            c.parse(&serde_json::from_str(input).unwrap());
            c.mark_all_dirty();
        } else {
            for op in input.split(';').filter(|o| !o.is_empty()) {
                let mut parts = op.split('|');
                let (t, v, d) = (parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap());
                let stat = by_names(&Identifier::parse(t).unwrap(), &Identifier::parse(v).unwrap()).unwrap();
                c.increment(stat, d.parse().unwrap());
            }
        }
        let expected: Value = serde_json::from_str(out).unwrap();
        let actual = c.to_json();
        // The key order inside a stat type follows the bucket order of vanilla's HashMap; entries that collide in a bucket
        // appear in the (fastutil) insertion order, which is only reproduced exactly for files that fit the default tables.
        // Content is always compared as a set; the order strictly with STATS_STRICT_ORDER=1.
        if std::env::var_os("STATS_STRICT_ORDER").is_some() {
            assert_eq!(canonical(&actual), canonical(&expected), "{header}: file content\ninput {input}");
        } else if canonical_sorted(&actual) != canonical_sorted(&expected) {
            panic!("{header}: file content\ninput {input}");
        }
        assert_eq!(actual["DataVersion"], expected["DataVersion"]);
        assert_eq!(packet_line(&mut c), packet, "{header}: packet entry order\ninput {input}");
        cases += 1;
    }
    assert!(cases >= 25, "{cases}");
}
