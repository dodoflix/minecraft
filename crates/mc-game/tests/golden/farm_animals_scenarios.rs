//! Farm-animal goldens (pig, cow, mooshroom, sheep, chicken: breeding, idling, milking, stew, shearing, grazing, variants) replayed
//! against the committed vanilla fixtures. The scenarios that need blocks, biomes or the ticking level run on the block replay.
use crate::common::block_scenario::check;
use crate::common::entity_scenario::*;

#[test]
fn mob_breed() {
    run_scenario_test("mob_breed");
}

#[test]
fn mob_animals_idle() {
    run_scenario_test("mob_animals_idle");
}

/// Feeding sets love, pairs walk to each other and breed, the babies follow their parents.
#[test]
fn mob_farm_breed_natural() {
    check("mob_farm_breed_natural");
}

/// Tempting with food, losing interest, panicking after a hit.
#[test]
fn mob_farm_tempt_panic() {
    check("mob_farm_tempt_panic");
}

/// A bucket on an adult cow or mooshroom: milk for survival and creative players alike, none from a baby.
#[test]
fn mob_cow_milk() {
    check("mob_cow_milk");
}

/// A bowl on a mooshroom, flowers on a brown one, shears, creative players.
#[test]
fn mob_mooshroom_stew() {
    check("mob_mooshroom_stew");
}

/// A saddled pig under a simulated rider: the boost curve, its expiry and the saddle rules.
#[test]
fn mob_pig_ride_boost() {
    check("mob_pig_ride_boost");
}

/// Sheep in pens eat grass and short grass: the goal's draws, the animation and what it changes.
#[test]
fn mob_sheep_eat_grass() {
    check("mob_sheep_eat_grass");
}

/// The baby's colour for dye pairs with and without a crafting recipe.
#[test]
fn mob_sheep_breed_colors() {
    check("mob_sheep_breed_colors");
}

/// Shearing: the drops and their three random floats each, babies and sheared sheep.
#[test]
fn mob_sheep_shear_drops() {
    check("mob_sheep_shear_drops");
}

/// Variants and wool colours chosen at spawn in several biomes.
#[test]
fn mob_farm_biome_variants() {
    check("mob_farm_biome_variants");
}

/// Loot conditions that read the variant components: sheep wool, laid eggs, mooshroom mushrooms.
#[test]
fn mob_variant_loot() {
    check("mob_variant_loot");
}

/// Chicken jockeys: no egg, 10 experience points, the jockey flag in the saved data.
#[test]
fn mob_chicken_jockey() {
    check("mob_chicken_jockey");
}

/// Entity predicates over the variant components of the farm and pet variant mobs.
#[test]
fn mob_variant_predicate() {
    check("mob_variant_predicate");
}
