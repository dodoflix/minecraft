//! Creeper, spider, cave spider, silverfish and endermite behaviour replayed against the vanilla oracle recordings
//! (`tests/oracle/scenarios/`, fixtures `tests/fixtures/entity_oracle/`, recorded with `scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

#[test]
fn charged_creeper_explodes_with_double_radius_and_leaves_a_cloud() {
    check("mob_creeper_charged");
}

#[test]
fn flint_steel_and_fire_charge_ignite_creepers() {
    check("mob_creeper_ignite");
}

#[test]
fn creeper_flees_cats_and_ignores_goats() {
    check("mob_creeper_cat_avoid");
}

#[test]
fn spider_climbs_leaps_and_gives_up_by_day() {
    check("mob_spider_attack");
}

#[test]
fn cave_spider_poison_follows_difficulty() {
    check("mob_cave_spider_poison");
}

#[test]
fn spider_jockeys_and_group_effects_on_hard() {
    check("mob_spider_jockey");
}

#[test]
fn silverfish_merge_with_stone_and_wake_friends() {
    check("mob_silverfish_merge_wake");
}

#[test]
fn endermite_pearl_spawn_and_lifetime() {
    check("mob_endermite_teleport");
}

#[test]
fn enderman_targets_endermites() {
    check("mob_endermite_enderman");
}
