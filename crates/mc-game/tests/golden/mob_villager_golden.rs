//! Villager behaviour against the vanilla oracle (ticking-level replay): panic and golem summoning, sleeping, farming, breeding,
//! restocking and job site claims. Scenarios: `tests/oracle/scenarios/mob_villager_*.txt`, fixtures: `tests/fixtures/entity_oracle/`
//! (`scripts/block-oracle.sh`).
use crate::common::block_scenario;

/// Two villagers with food and beds breed after a breed target is set: love duration, bed claim, baby, parents' age and food.
#[test]
#[ignore = "deviation: the baby's first walk target after the birth differs (`walk_target` of the baby, `interaction_target=villager#1` vs `V2` on `BM V1`, fixture line 142); the level random is one draw behind from the birth tick"]
fn mob_villager_breed() {
    block_scenario::check("mob_villager_breed");
}

/// Three villagers that slept recently agree to an iron golem on a panic tick after a zombie hit one of them.
#[test]
fn mob_villager_golem_spawn() {
    block_scenario::check("mob_villager_golem_spawn");
}

/// Two unemployed villagers claim two free workstations: potential job site, claim, profession change.
#[test]
fn mob_villager_job_claim() {
    block_scenario::check("mob_villager_job_claim");
}

/// A zombie hunts a villager: panic activity, flight, calming down.
#[test]
#[ignore = "deviation: the villager's position differs from tick 95 (fixture line 215, `E V`) and `look_target=entity(Steve)` is missing a few ticks later"]
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
#[ignore = "deviation: the wanted-item walk target after a harvest starts one tick late (fixture line 46, `look_target=pos(3,-60,2)` vs `pos(3,-60,1)`); the level random is one draw behind from tick 89"]
fn mob_villager_work_farm() {
    block_scenario::check("mob_villager_work_farm");
}
