//! Differential test of `PrimaryLevelData` against the vanilla classes: `fixtures/level_storage/level_dat_golden.txt`
//! is produced by `fixtures/level_storage/LevelDatGolden.java` (real `LevelSettings.parse`, `LevelVersion.parse`,
//! `PrimaryLevelData.parse/createTag`, `FeatureFlags.CODEC`).

use mc_nbt::snbt;
use mc_world::level_storage::{FeatureFlagSet, PrimaryLevelData, SpecialWorldProperty};

const GOLDEN: &str = include_str!("../fixtures/level_storage/level_dat_golden.txt");

fn list(items: &[String]) -> String {
    format!("[{}]", items.join(", "))
}

fn describe(p: &PrimaryLevelData, lv: &mc_world::level_storage::LevelVersion) -> Vec<String> {
    let s = &p.settings;
    let packs = s.data_configuration.data_packs();
    let mut features: Vec<String> = s
        .data_configuration
        .enabled_features
        .names()
        .iter()
        .map(|i| i.to_string())
        .collect();
    features.sort();
    let mut removed: Vec<String> = p.removed_feature_flags.iter().cloned().collect();
    removed.sort();
    let sp = &p.respawn_data;
    vec![
        format!("levelName={}", s.level_name),
        format!("gameType={}", s.game_type.get_id()),
        format!(
            "difficulty={},{},{}",
            s.difficulty_settings.difficulty.get_serialized_name(),
            s.difficulty_settings.hardcore,
            s.difficulty_settings.locked
        ),
        format!("allowCommands={}", s.allow_commands),
        format!(
            "dataPacks={}|{}",
            list(&packs.enabled),
            list(&packs.disabled)
        ),
        format!(
            "features={} experimental={}",
            list(&features),
            s.data_configuration.enabled_features.is_experimental()
        ),
        format!(
            "spawn={},{},{},{},{:x},{:x}",
            sp.dimension,
            sp.pos.x(),
            sp.pos.y(),
            sp.pos.z(),
            sp.yaw.to_bits(),
            sp.pitch.to_bits()
        ),
        format!("time={}", p.game_time),
        format!(
            "singleplayer={}",
            p.single_player_uuid
                .map_or("null".to_owned(), |u| u.to_string())
        ),
        format!(
            "version={} initialized={} wasModded={}",
            p.version, p.initialized, p.was_modded
        ),
        format!(
            "brands={}",
            list(&p.known_server_brands.iter().cloned().collect::<Vec<_>>())
        ),
        format!("removed={}", list(&removed)),
        format!(
            "levelVersion={},{},{},{},{},{}",
            lv.level_data_version,
            lv.last_played,
            lv.minecraft_version_name,
            lv.minecraft_version.version,
            lv.minecraft_version.series,
            lv.snapshot
        ),
    ]
}

#[test]
fn parse_and_reserialize_match_vanilla() {
    let mut cases = 0;
    let mut name = String::new();
    let mut input = None;
    let mut expected_fields: Vec<String> = Vec::new();
    for line in GOLDEN.lines() {
        if let Some(n) = line.strip_prefix("@@ ") {
            name = n.to_owned();
            expected_fields.clear();
            input = None;
        } else if let Some(i) = line.strip_prefix("IN ") {
            input = Some(
                snbt::parse_compound(i)
                    .unwrap_or_else(|e| panic!("{name}: bad IN snbt: {}", e.message())),
            );
        } else if let Some(o) = line.strip_prefix("OUT ") {
            let data = input.take().expect("IN before OUT");
            let expected_out = snbt::parse_compound(o).unwrap();
            let parsed = PrimaryLevelData::parse(&data, SpecialWorldProperty::None);
            let level_version = mc_world::level_storage::LevelVersion::parse(&data);
            assert_eq!(
                describe(&parsed, &level_version),
                expected_fields,
                "case {name}: parsed fields"
            );

            let mut out = parsed.to_tag_at(None, 0);
            // vanilla's parse drops these; ours preserves them (documented deviation) — compare without them.
            for key in ["Player", "WorldGenSettings", "custom_unknown"] {
                if data.contains(key) {
                    assert_eq!(
                        out.remove(key).as_ref(),
                        data.get(key),
                        "case {name}: extra key {key} preserved"
                    );
                }
            }
            assert_eq!(
                out,
                expected_out,
                "case {name}: re-serialized tag\n ours: {}\n vanilla: {}",
                mc_nbt::Tag::Compound(out.clone()),
                mc_nbt::Tag::Compound(expected_out.clone())
            );
            cases += 1;
        } else if let Some(f) = line.strip_prefix("FLAGS ") {
            let mask: u32 = name.strip_prefix("flags_").unwrap().parse().unwrap();
            let bits = [
                FeatureFlagSet::VANILLA,
                FeatureFlagSet::TRADE_REBALANCE,
                FeatureFlagSet::REDSTONE_EXPERIMENTS,
                FeatureFlagSet::MINECART_IMPROVEMENTS,
            ];
            let set = (0..4)
                .filter(|i| mask >> i & 1 != 0)
                .fold(FeatureFlagSet::EMPTY, |acc, i| acc.join(bits[i]));
            assert_eq!(set.encode().to_string(), f, "flags mask {mask}");
            cases += 1;
        } else if !line.is_empty() {
            expected_fields.push(line.to_owned());
        }
    }
    assert!(
        cases >= 7 + 16,
        "golden file seems truncated: {cases} cases"
    );
}
