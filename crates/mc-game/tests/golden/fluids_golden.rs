//! Fluid spreading: closed-form checks on the real fluid code, then replays of the vanilla recordings
//! (scenarios `tests/oracle/scenarios/blk_fluid_*.txt`, fixtures from `scripts/block-oracle.sh NAME`).
use crate::common::block_scenario::{check, check_with, run_scenario_text};
use crate::common::entity_oracle::Options;

#[test]
fn water_source_on_flat_floor_spreads_seven_blocks() {
    let out = run_scenario_text(
        "level flat\npreload 2\nblock 0 -60 0 minecraft:water[level=0]\ntick 45\ndump blocks:-9,-60,-9,9,-60,9\n",
    );
    for x in -9i32..=9 {
        for z in -9i32..=9 {
            let d = x.abs() + z.abs();
            let prefix = format!("B {x} -60 {z} ");
            let state = out
                .iter()
                .find_map(|l| l.strip_prefix(&prefix))
                .map(String::from);
            let want = match d {
                0 => Some("minecraft:water[level=0]".to_string()),
                1..=7 => Some(format!("minecraft:water[level={d}]")),
                _ => None,
            };
            assert_eq!(state, want, "at {x} {z}");
        }
    }
}

#[test]
fn lava_source_above_water_turns_the_water_into_stone() {
    let out = run_scenario_text(
        "level flat\npreload 2\nblock 0 -60 0 minecraft:water[level=0]\nblock 0 -59 0 minecraft:lava[level=0]\ntick 40\ndump blocks:0,-60,0,0,-59,0\n",
    );
    assert_eq!(
        out,
        [
            "B 0 -60 0 minecraft:stone",
            "B 0 -59 0 minecraft:lava[level=0]"
        ]
    );
}

macro_rules! scenario {
    ($($name:ident),*) => {$(
        #[test]
        fn $name() {
            check(concat!("blk_fluid_", stringify!($name)));
        }
    )*};
}

scenario!(water_flat, water_pit, lava_flat, lava_water, waterlog);

/// The `gamerule` verb prints the vanilla command feedback line (`C ...`), which the replay does not model.
fn check_without_feedback(name: &str) {
    check_with(
        name,
        &Options {
            ignore_kinds: vec!["C"],
            ..Options::default()
        },
    );
}

#[test]
fn water_infinite() {
    check_without_feedback("blk_fluid_water_infinite");
}

#[test]
fn lava_fire() {
    check_without_feedback("blk_fluid_lava_fire");
}
