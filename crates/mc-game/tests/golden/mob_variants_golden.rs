//! Spawn variants on a live level: the biome, moon phase and structure the spawn context reads when natural spawns, spawn eggs and commands pick
//! the variant of wolves, cats, pigs, cows, chickens, frogs and rabbits. Scenarios: `tests/oracle/scenarios/mob_variant_*.txt`,
//! fixtures: `tests/fixtures/entity_oracle/` (`scripts/entity-oracle.sh`).
use crate::common::block_scenario::{check, check_with};
use crate::common::entity_oracle::Options;

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

/// A cat on a generated swamp hut's pieces, beside it and above it; the hut's own witch and cat are dumped as well. The witch's
/// head and body rotation are not compared (and no other mob's fields are skipped): a chunk-generated mob's random yaw and look
/// timing depend on the creation order of the generated chunks.
#[test]
fn variant_structure() {
    check_with(
        "mob_variant_structure",
        &Options {
            ignore_fields_of: vec![("minecraft:witch", "head"), ("minecraft:witch", "body")],
            ..Options::default()
        },
    );
}

/// A swamp hut placed by command on a flat level: no structure references exist, so its cat and cats on its pieces pick like any cat.
#[test]
fn variant_place_structure() {
    check("mob_variant_place_structure");
}
