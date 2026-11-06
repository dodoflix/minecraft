//! Goldens of the generic combat/target goals (`ai/goal/target/*`, `MeleeAttackGoal`, `SwellGoal`, ...) through real vanilla mobs
//! that use them: the fixtures are produced by the in-process vanilla oracle (`scripts/entity-oracle.sh`).
use crate::common::entity_oracle::Options;
use crate::common::entity_scenario::run_scenario_test_with;

/// Creeper: `NearestAttackableTargetGoal(Player, mustSee)` (randomInterval RNG, line of sight), `HurtByTargetGoal`, `MeleeAttackGoal`
/// path recompute RNG, `SwellGoal` fuse timing.
#[test]
fn creeper_target_swell_melee() {
    run_scenario_test_with("mob_creeper", &Options::default());
}
