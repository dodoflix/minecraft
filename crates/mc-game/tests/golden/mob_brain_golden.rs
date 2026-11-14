//! Sensors, attribute suppliers and brain persistence of living mobs against the vanilla oracle (ticking-level replay).
//! Scenarios: `tests/oracle/scenarios/mob_{sensor_grid,attrs_all,brain_nbt}.txt`, fixtures: `tests/fixtures/entity_oracle/`
//! (`scripts/block-oracle.sh`).
use crate::common::block_scenario;

/// A villager's sensors over a grid of golem, zombie, baby, item and player: scan cadence and every memory they write.
#[test]
fn mob_sensor_grid() {
    block_scenario::check("mob_sensor_grid");
}

/// The attribute supplier of every living entity type, and attribute base values/modifiers through save and load.
#[test]
fn mob_attrs_all() {
    block_scenario::check("mob_attrs_all");
}

/// Brain memories with time to live of a villager and a piglin after 200 ticks, their saved `Brain` tag, and the tag loaded back.
#[test]
fn mob_brain_nbt() {
    block_scenario::check("mob_brain_nbt");
}
