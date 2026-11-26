//! Goldens of the amphibians: axolotl hunting, playing dead, land behaviour and variants, frog tongue, jumping and variants by
//! biome temperature, tadpole growth into a frog, the axolotl and tadpole buckets and the variant component predicates.
//! Scenarios: `tests/oracle/scenarios/{mob_axolotl_*,mob_frog_*,mob_tadpole_grow,mob_amphibian_*}.txt`,
//! fixtures: `tests/fixtures/entity_oracle/` (`scripts/entity-oracle.sh`).
use crate::common::block_scenario::check;

#[test]
fn axolotl_hunt_playdead_matches_vanilla() {
    check("mob_axolotl_hunt_playdead");
}

#[test]
fn axolotl_land_matches_vanilla() {
    check("mob_axolotl_land");
}

#[test]
fn axolotl_breed_variants_matches_vanilla() {
    check("mob_axolotl_breed_variants");
}

#[test]
fn frog_tongue_slime_matches_vanilla() {
    check("mob_frog_tongue_slime");
}

#[test]
fn frog_jump_matches_vanilla() {
    check("mob_frog_jump");
}

#[test]
fn frog_variant_temperature_matches_vanilla() {
    check("mob_frog_variant_temperature");
}

#[test]
fn tadpole_grow_matches_vanilla() {
    check("mob_tadpole_grow");
}

/// A tadpole loaded with an Age of 24000 or more becomes a frog while loading (summon, data merge).
#[test]
fn tadpole_load_age_matches_vanilla() {
    check("mob_tadpole_load_age");
}

/// A water bucket on every axolotl variant, a baby and the tadpoles picks the mob up, the filled bucket releases it again.
#[test]
fn amphibian_bucket_matches_vanilla() {
    check("mob_amphibian_bucket");
}

/// Entity predicates over the frog and axolotl variant components.
#[test]
fn amphibian_predicate_matches_vanilla() {
    check("mob_amphibian_predicate");
}

#[test]
fn tadpole_ignores_the_saved_from_bucket_flag() {
    check("mob_tadpole_from_bucket");
}
