//! `GameRules`/`GameRuleMap` against the real vanilla classes (`fixtures/GameRuleGolden.java` -> `gamerule.golden`).

use std::collections::BTreeMap;

use mc_game::world::level::gamerules::{
    AnyGameRule, GameRuleMap, GameRuleType, GameRuleValue, GameRules,
};
use mc_world::level_storage::FeatureFlagSet;

fn golden() -> Vec<Vec<String>> {
    include_str!("../../../mc-game/tests/fixtures/gamerule.golden")
        .lines()
        .map(|l| l.split('\t').map(str::to_string).collect())
        .collect()
}

fn java_value(v: GameRuleValue) -> String {
    match v {
        GameRuleValue::Bool(b) => b.to_string(),
        GameRuleValue::Int(i) => i.to_string(),
    }
}

/// `TreeMap.toString()` of the rule map keyed by short id.
fn java_map(map: &GameRuleMap) -> String {
    let sorted: BTreeMap<String, String> = map
        .key_set()
        .map(|r| (r.id(), java_value(map.get_value(r).unwrap())))
        .collect();
    format!(
        "{{{}}}",
        sorted
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn rule(id: &str) -> AnyGameRule {
    AnyGameRule::by_identifier(&mc_core::ident::Identifier::parse(id).unwrap())
        .unwrap_or_else(|| panic!("unknown rule {id}"))
}

#[test]
fn rule_table_matches_vanilla() {
    let rows: Vec<_> = golden().into_iter().filter(|r| r[0] == "RULE").collect();
    assert_eq!(rows.len(), AnyGameRule::all().count());
    for (r, row) in AnyGameRule::all().zip(&rows) {
        let ctx = &row[1];
        assert_eq!(r.id(), row[1], "registry order");
        assert_eq!(r.get_identifier().to_string(), row[2], "{ctx}");
        assert_eq!(
            match r.game_rule_type() {
                GameRuleType::Bool => "BOOL",
                GameRuleType::Int => "INT",
            },
            row[3],
            "{ctx}"
        );
        assert_eq!(java_value(r.default_value()), row[4], "{ctx}");
        assert_eq!(r.category().id().to_string(), row[5], "{ctx}");
        assert_eq!(r.get_description_id(), row[6], "{ctx}");
        let expected_features = if row[8] == "[]" {
            FeatureFlagSet::EMPTY
        } else {
            FeatureFlagSet::MINECART_IMPROVEMENTS
        };
        assert_eq!(r.required_features(), expected_features, "{ctx}");
        let (cmd_ser, cmd_res) = row[9].split_once('/').unwrap();
        assert_eq!(r.serialize(r.default_value()), cmd_ser, "{ctx}");
        assert_eq!(
            r.get_command_result(r.default_value()).to_string(),
            cmd_res,
            "{ctx}"
        );
        let arg = r.argument::<()>().describe();
        // brigadier's toString: `bool()` / `integer(min)` / `integer(min, max)`
        match r.game_rule_type() {
            GameRuleType::Bool => assert_eq!(row[7], "BoolArgumentType", "{ctx}"),
            GameRuleType::Int => {
                let (min, max) = row[7]
                    .strip_prefix("integer(")
                    .unwrap()
                    .trim_end_matches(')')
                    .split_once(',')
                    .unwrap();
                let expect = if max == "2147483647" {
                    format!("integer({min})")
                } else {
                    format!("integer({min}, {max})")
                };
                assert_eq!(arg, expect, "{ctx}");
            }
        }
    }
}

#[test]
fn deserialize_matches_vanilla() {
    let mut n = 0;
    for row in golden().iter().filter(|r| r[0] == "DESER") {
        let r = rule(&format!("minecraft:{}", row[1]));
        let input = row[2].strip_prefix('[').unwrap().strip_suffix(']').unwrap();
        let (result, partial, err) = (
            row[3].strip_prefix("result=").unwrap(),
            row[4].strip_prefix("partial=").unwrap(),
            row[5].strip_prefix("err=").unwrap(),
        );
        let got = r.deserialize(input);
        let ctx = format!("{} [{input}]", row[1]);
        match &got {
            Ok(v) => {
                assert_eq!(
                    (java_value(*v).as_str(), java_value(*v).as_str(), ""),
                    (result, partial, err),
                    "{ctx}"
                );
            }
            Err(p) => {
                assert_eq!(result, "none", "{ctx}");
                assert_eq!(
                    p.value.map_or("none".to_string(), java_value),
                    partial,
                    "{ctx}"
                );
                assert_eq!(p.error, err, "{ctx}");
            }
        }
        n += 1;
    }
    assert_eq!(n, 59 * 24);
}

#[test]
fn codec_decode_matches_vanilla() {
    for row in golden().iter().filter(|r| r[0] == "CODEC") {
        let tag = mc_nbt::snbt::parse(&row[1]).unwrap_or_else(|e| panic!("{}: {e:?}", row[1]));
        let (result, partial, err) = (
            row[2].strip_prefix("result=").unwrap(),
            row[3].strip_prefix("partial=").unwrap(),
            row[4].strip_prefix("err=").unwrap(),
        );
        match GameRuleMap::decode(&tag) {
            Ok(m) => assert_eq!(
                (java_map(&m).as_str(), java_map(&m).as_str(), ""),
                (result, partial, err),
                "{}",
                row[1]
            ),
            Err(p) => {
                assert_eq!(result, "none", "{}", row[1]);
                assert_eq!(
                    p.value.as_ref().map_or("none".to_string(), java_map),
                    partial,
                    "{}",
                    row[1]
                );
                assert!(!err.is_empty(), "{}", row[1]);
            }
        }
    }
}

#[test]
fn codec_encode_matches_vanilla() {
    let rows = golden();
    let get = |k: &str| rows.iter().find(|r| r[0] == k).unwrap()[1].clone();
    let default = GameRuleMap::of(AnyGameRule::all());
    assert_eq!(
        default.encode(),
        mc_nbt::snbt::parse(&get("ENCODE_DEFAULT")).unwrap()
    );
    let mut alt = GameRuleMap::of_empty();
    for r in AnyGameRule::all() {
        alt.set_value(
            r,
            match r.default_value() {
                GameRuleValue::Bool(b) => GameRuleValue::Bool(!b),
                GameRuleValue::Int(i) => GameRuleValue::Int(if i == 1 { 2 } else { 1 }),
            },
        );
    }
    assert_eq!(
        alt.encode(),
        mc_nbt::snbt::parse(&get("ENCODE_ALT")).unwrap()
    );
    // and back
    assert_eq!(GameRuleMap::decode(&alt.encode()).unwrap(), alt);
}

#[test]
fn feature_filtering_matches_vanilla() {
    let rows = golden();
    let count = |k: &str| {
        rows.iter().find(|r| r[0] == k).unwrap()[1]
            .parse::<usize>()
            .unwrap()
    };
    assert_eq!(
        GameRules::new(FeatureFlagSet::EMPTY)
            .available_rules()
            .count(),
        count("RULES_VANILLA")
    );
    assert_eq!(
        GameRules::new(FeatureFlagSet::ALL)
            .available_rules()
            .count(),
        count("RULES_ALL")
    );
    assert_eq!(
        GameRules::new(FeatureFlagSet::MINECART_IMPROVEMENTS)
            .available_rules()
            .count(),
        count("RULES_MINECART_ONLY")
    );
    // a map that holds the gated rule loses it when the feature is off, and missing rules are reset to defaults
    let mut m = GameRuleMap::of_empty();
    m.set(GameRules::MAX_MINECART_SPEED, 9);
    m.set(GameRules::PVP, false);
    let rules = GameRules::from_map(FeatureFlagSet::EMPTY, m);
    assert!(!rules.map().has(GameRules::MAX_MINECART_SPEED.erase()));
    assert!(!rules.get(GameRules::PVP));
    assert_eq!(rules.get(GameRules::RANDOM_TICK_SPEED), 3);
}
