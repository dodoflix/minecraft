//! Spawn variants on a live level: the biome, moon phase and structure the spawn context reads when natural spawns, spawn eggs and commands pick
//! the variant of wolves, cats, pigs, cows, chickens, frogs and rabbits. Scenarios: `tests/oracle/scenarios/mob_variant_*.txt`,
//! fixtures: `tests/fixtures/entity_oracle/` (`scripts/entity-oracle.sh`).
use crate::common::block_scenario::check;

/// Natural-reason and spawn-egg spawns over sixteen biomes, and mobs above the filled biome volume.
#[test]
fn variant_biomes() {
    check("mob_variant_biomes");
}

/// The cat variant on every moon phase, around the day boundaries.
#[test]
fn variant_moon() {
    check("mob_variant_moon");
}

/// A cat on a generated swamp hut's pieces, beside it and above it.
#[test]
fn variant_structure() {
    check("mob_variant_structure");
}
