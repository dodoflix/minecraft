//! Polar bears against the vanilla oracle (ticking-level replay): defending a cub, standing up before the swing, alerting adults when a cub
//! is hurt, persistent anger and attacking foxes.
//! Scenarios: `tests/oracle/scenarios/mob_polar_bear_*.txt`, fixtures: `tests/fixtures/entity_oracle/` (`scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

/// Adults attack players near their cub, forget them when far away, answer a hurt cub, stay angry after being hit and hunt foxes.
#[test]
fn polar_bears_defend_cubs_and_get_angry() {
    check("mob_polar_bear_anger_cub");
}
