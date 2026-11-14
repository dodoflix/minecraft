//! Rabbits against the vanilla oracle (ticking-level replay): the variant rolls of every biome, groups and breeding, the variant entity
//! predicate, raiding a carrot garden, fleeing and panicking, and the killer bunny.
//! Scenarios: `tests/oracle/scenarios/mob_rabbit_*.txt`, fixtures: `tests/fixtures/entity_oracle/` (`scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

/// `execute if predicate` over the rabbit variant component, killer bunny included.
#[test]
fn rabbit_variant_predicate() {
    check("mob_rabbit_predicate");
}

/// Variants chosen by biome on spawn, shared inside a spawn group, inherited when breeding, and the killer bunny's attributes.
#[test]
fn rabbit_variants_by_biome_group_and_breeding() {
    check("mob_rabbit_variants");
}

/// A rabbit walks to a full-grown carrot, eats a growth stage and waits before eating again; nothing happens without mob griefing.
#[test]
fn rabbit_raids_carrots() {
    check("mob_rabbit_raid");
}

/// Hopping, avoiding a survival player, a wolf and a monster, panic when hurt, and tempting with a carrot.
#[test]
fn rabbit_hops_avoids_and_panics() {
    check("mob_rabbit_avoid_panic");
}

/// The killer bunny attacks players and wolves, and its pack answers when it is hurt.
#[test]
fn killer_bunny_attacks() {
    check("mob_rabbit_killer");
}
