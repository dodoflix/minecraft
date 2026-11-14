//! Villager behaviour against the vanilla oracle (ticking-level replay): panic and golem summoning, sleeping, farming, breeding,
//! restocking, job site claims and gifts to a hero of the village. Scenarios: `tests/oracle/scenarios/mob_villager_*.txt`, fixtures: `tests/fixtures/entity_oracle/`
//! (`scripts/block-oracle.sh`).
use crate::common::block_scenario;

/// Two villagers with food and beds breed after a breed target is set: love duration, bed claim, baby, parents' age and food.
#[test]
fn mob_villager_breed() {
    block_scenario::check("mob_villager_breed");
}

/// Three villagers that slept recently agree to an iron golem on a panic tick after a zombie hit one of them.
#[test]
fn mob_villager_golem_spawn() {
    block_scenario::check("mob_villager_golem_spawn");
}

/// An unemployed villager in a pen with a hero of the village throws its gift after the cool-down.
#[test]
fn mob_villager_gift() {
    block_scenario::check("mob_villager_gift");
}

/// Two unemployed villagers claim two free workstations: potential job site, claim, profession change.
#[test]
fn mob_villager_job_claim() {
    block_scenario::check("mob_villager_job_claim");
}

/// A zombie hunts a villager: panic activity, flight, calming down.
#[test]
fn mob_villager_panic_zombie() {
    block_scenario::check("mob_villager_panic_zombie");
}

/// A librarian with used-up trades restocks at its lectern in the workday.
#[test]
fn mob_villager_restock() {
    block_scenario::check("mob_villager_restock");
}

/// A villager walks to its bed at nightfall, sleeps and gets up in the morning.
#[test]
fn mob_villager_sleep() {
    block_scenario::check("mob_villager_sleep");
}

/// A farmer harvests and plants around its composter.
#[test]
fn mob_villager_work_farm() {
    block_scenario::check("mob_villager_work_farm");
}
