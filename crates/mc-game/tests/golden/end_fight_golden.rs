//! The End level's dragon fight in the scenario DSL (`end fight`): `tests/oracle/scenarios/end_fight_*.txt`, fixtures from `scripts/block-oracle.sh NAME`
//! (the real vanilla server with the fight kept). The verbs and dumps are in `FightVerbs.java` and `common/block_scenario_fight.rs`.
use crate::common::block_scenario::check;

/// `fighthash` over the arena and the gateway sites and `fightpackets:` (boss bar and level events with the bar uuid zeroed) in a level without a fight.
#[test]
fn fight_dumps_without_a_fight() {
    check("end_fight_dumps");
}

/// The fight right after boot (shuffled gateways, a scan still to run, the saved file), its first scan in a void End, a kill, a second kill and the
/// start of the respawn ritual.
#[test]
#[ignore = "deviation: the dragon fight (EnderDragonFight) is not ported (#14)"]
fn boot_scan_kill_and_respawn_in_a_void_end() {
    check("end_fight_boot");
}

/// The first scan over the real End terrain: podium position, ten pillar crystals, the dragon.
#[test]
#[ignore = "deviation: the dragon fight (EnderDragonFight) is not ported (#14)"]
fn first_scan_over_the_real_end() {
    check("end_fight_real_end");
}
