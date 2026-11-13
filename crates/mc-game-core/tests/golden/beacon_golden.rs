//! `BeaconBlockEntity.validateEffects` and the `BeaconMenu` effect encoding against the vanilla classes
//! (`fixtures/BeaconGolden.java`; the Java side maps a vanilla `NullPointerException` to "invalid", `None` here).

use mc_data::MobEffect;
use mc_game::world::inventory::{decode_effect, encode_effect, validate_effects};

#[test]
fn validate_effects_matches_vanilla() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../mc-game/tests/fixtures/beacon_golden.txt"
    ))
    .expect("fixture");
    let mut n = 0;
    for line in text.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        let (p, s, levels): (i32, i32, i32) = (
            f[0].parse().unwrap(),
            f[1].parse().unwrap(),
            f[2].parse().unwrap(),
        );
        let want: bool = f[3].parse().unwrap();
        let (primary, secondary): (Option<MobEffect>, Option<MobEffect>) =
            (decode_effect(p), decode_effect(s));
        assert_eq!(encode_effect(primary), p);
        assert_eq!(encode_effect(secondary), s);
        assert_eq!(
            validate_effects(primary, secondary, levels).unwrap_or(false),
            want,
            "primary {p} secondary {s} levels {levels}"
        );
        n += 1;
    }
    assert!(n > 1000);
}
