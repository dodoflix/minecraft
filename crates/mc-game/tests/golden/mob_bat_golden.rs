//! Bats against the vanilla oracle (ticking-level replay): hanging under a ceiling, waking for a player, flying and roosting again.
//! Scenarios: `tests/oracle/scenarios/mob_bat_*.txt`, fixtures: `tests/fixtures/entity_oracle/` (`scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

/// Resting bats wake when a player comes near, the ceiling goes away or they are hurt; awake bats fly around and hang up again.
#[test]
fn bats_roost_and_fly() {
    check("mob_bat_roost");
}
