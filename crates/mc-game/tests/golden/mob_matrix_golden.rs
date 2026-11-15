//! Matrices over many mob classes against the vanilla oracle (ticking-level replay): loot and experience of every spawnable type, natural
//! spawn attempts on three surfaces in eight biomes, spawn passes against light and thunder, random despawn, breeding of 66 parent pairs,
//! zombie conversions, villager prices and reputation, the wither boss bar, a raid that loses its village, item pickup and a sweep attack
//! beside pets. Scenarios: `tests/oracle/scenarios/mob_*.txt`, fixtures: `tests/fixtures/entity_oracle/` (`scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

#[test]
fn every_spawnable_type_drops_its_loot_and_experience() {
    check("mob_loot_matrix");
}

#[test]
fn natural_spawn_attempts_on_grass_sand_and_snow_in_eight_biomes() {
    check("mob_spawn_surfaces");
}

#[test]
fn monster_spawn_passes_follow_block_light_and_thunder() {
    check("mob_spawn_light");
}

#[test]
fn mobs_far_from_players_despawn_at_random() {
    check("mob_despawn_random");
}

#[test]
fn sixty_six_parent_pairs_breed_their_offspring() {
    check("mob_breed_matrix");
}

#[test]
fn zombies_convert_and_infect_on_every_difficulty() {
    check("mob_zombie_convert");
}

#[test]
fn villager_prices_follow_reputation_and_the_hero_of_the_village() {
    check("mob_villager_price");
}

#[test]
fn wither_boss_bar_follows_tracking_damage_and_death() {
    check("mob_wither_bossbar_tracking");
}

#[test]
fn raid_without_its_village_and_a_second_raid() {
    check("mob_raid_center");
}

#[test]
fn mobs_pick_up_and_equip_dropped_items() {
    check("mob_pickup_equip");
}

#[test]
fn sweep_attack_spares_the_pets_beside_the_target() {
    check("mob_sweep_pet");
}

#[test]
fn player_interactions_with_mobs_match_vanilla() {
    check("mob_interact_matrix");
}
