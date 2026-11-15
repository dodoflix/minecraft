//! Advancement criteria fired by fights against the vanilla oracle (ticking-level replay, data pack `tests/oracle/datapacks/adv_combat`):
//! damage dealt and taken, kills, arrows, crossbows, spears, totems, explosion falls and lightning. Scenarios: `tests/oracle/scenarios/adv_*.txt`,
//! fixtures: `tests/fixtures/entity_oracle/` (`scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

/// Melee, projectile and fire damage to mobs, players behind a shield, interaction entities and armor stands: the damage conditions of
/// the trigger, the damage window and the damage before the shield.
#[test]
fn hurting_entities_awards_the_attacker() {
    check("adv_hurt_entity");
}

/// A zombie's bite, deflected and landed arrows, fire, explosions, the damage window, armor, absorption and creative players.
#[test]
fn hurting_players_awards_the_victim() {
    check("adv_hurt_player");
}

/// Kills by melee, arrows, fire after an attack, explosions, players and mobs killing players, a totem, a player named as its own killer.
#[test]
fn killing_awards_killer_and_victim() {
    check("adv_kill_triggers");
}

/// A mob killed near a sculk catalyst: who is credited, which blow the trigger sees, the experience-less bat and the range.
#[test]
fn kills_near_a_sculk_catalyst_award_the_attacker() {
    check("adv_sculk_kills");
}

/// Plain, bow, piercing and multishot arrows with their victims, unique types and weapon conditions, rockets, and the shot itself.
#[test]
fn arrows_and_crossbows_award_the_shooter() {
    check("adv_arrow_kills");
}

/// Totems in either hand and bypassing damage, and the fall after an explosion (the start position, the cause, the distance).
#[test]
fn totems_and_explosion_falls_award_the_player() {
    check("adv_totem_explosion");
}

/// A sprinting spear counts the living entities it stabbed; the stab action does not fire.
#[test]
fn spear_charges_count_stabbed_mobs() {
    check("adv_spear_mobs");
}

/// Bolts and the players around them: everyone within reach is awarded, the bolt's struck entities and the bystanders of its box.
#[test]
fn lightning_strikes_award_players_in_reach() {
    check("adv_lightning_strike");
}
