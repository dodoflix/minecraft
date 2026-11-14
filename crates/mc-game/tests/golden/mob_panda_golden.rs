//! Pandas against the vanilla oracle (ticking-level replay): genes and breeding, sneezing, rolling, sitting and thunder, bamboo and
//! fighting back.
//! Scenarios: `tests/oracle/scenarios/mob_panda_*.txt`, fixtures: `tests/fixtures/entity_oracle/` (`scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

/// Gene rolls on spawn, every personality, saved genes, attributes, the breeding call and breeding that needs bamboo nearby.
#[test]
fn panda_genes_and_breeding() {
    check("mob_panda_genes_breed");
}

/// Baby pandas sneeze, grown pandas nearby jump and a gift drops.
#[test]
fn panda_sneezes() {
    check("mob_panda_sneeze");
}

/// Rolling, lying on the back, worried pandas fleeing and sitting in thunder.
#[test]
fn panda_rolls_sits_and_worries() {
    check("mob_panda_roll_sit");
}

/// Feeding bamboo, eating on the ground, ageing up a baby and aggressive pandas fighting back.
#[test]
fn panda_eats_bamboo_and_fights() {
    check("mob_panda_bamboo");
}
