//! Probes recorded from the real vanilla level by the independent S1 audit (`status/scopes/S1-audit-independent.md`); all four fail
//! on main at 5e172210. Adopt: copy scenarios to `crates/mc-game/tests/oracle/scenarios/`, fixtures to
//! `crates/mc-game/tests/fixtures/entity_oracle/`, this file to `crates/mc-game/tests/golden/` (+ `mod` line in `golden/main.rs`).
use crate::common::block_scenario::check;

#[test]
fn trapped_chest_opened_by_a_player_powers_dust_and_lamp() {
    check("blk_audit_trapped");
}
#[test]
fn opening_shulker_box_pops_torch_on_top_and_chest_sounds() {
    check("blk_audit_misc");
}
#[test]
fn beacon_skips_spectators() {
    check("blk_audit_beacon");
}
#[test]
fn creative_break_of_unopened_loot_shulker_drops_the_box() {
    check("blk_audit_shulker_loot");
}
