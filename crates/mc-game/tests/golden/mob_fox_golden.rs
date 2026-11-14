//! Foxes against the vanilla oracle (ticking-level replay): sleeping and pouncing, berries and held items, trust and defending,
//! variants by biome, and breeding.
//! Scenarios: `tests/oracle/scenarios/mob_fox_*.txt`, fixtures: `tests/fixtures/entity_oracle/` (`scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

/// Sleeping under a roof by day, waking when hit, stalking and pouncing on chickens and rabbits, diving into a snow layer.
#[test]
fn fox_sleeps_and_pounces() {
    check("mob_fox_sleep_pounce");
}

/// Foxes walk to ripe sweet berry bushes and glow berries and pick them, eat what they carry, pick up and spit out items, and leave the
/// bushes alone without mob griefing.
#[test]
fn fox_picks_berries_and_items() {
    check("mob_fox_berries");
}

/// Snow in the snowy biomes, red elsewhere, shared by a spawn group with kits among its later members; saved type, trusted list, target
/// goals by variant and the variant predicate.
#[test]
fn fox_variants_by_biome() {
    check("mob_fox_variant_biome");
}

/// Berries and love, breeding with one or two love causes, kit variants and trust.
#[test]
fn fox_breeds() {
    check("mob_fox_breed");
}

/// A trusted fox stays and defends its player, untrusted ones run from players, wolves and polar bears.
#[test]
fn fox_trusts_and_defends() {
    check("mob_fox_trust_defend");
}
