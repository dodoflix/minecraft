//! The zombie family against the vanilla oracle (ticking-level replay): infecting villagers, curing zombie villagers, the spear,
//! drowned tridents, husk and zombified piglin behaviour. Scenarios: `tests/oracle/scenarios/mob_{zombie,drowned,husk,zombified}_*.txt`,
//! fixtures: `tests/fixtures/entity_oracle/` (`scripts/block-oracle.sh`).
use crate::common::block_scenario;

/// A zombie that kills a villager infects it (a coin flip on Normal, always on Hard, never on Easy) and the zombie villager keeps the
/// villager's data.
#[test]
fn zombie_infects_villagers() {
    block_scenario::check("mob_zombie_infect");
}

/// A golden apple on a weakened zombie villager cures it after 3600 to 6000 ticks; the villager keeps its data and thanks the player.
#[test]
fn zombie_villager_cure() {
    block_scenario::check("mob_zombie_villager_cure");
}

/// A drowned with a trident throws it at a player on the shore; one without swims after a player in the water.
#[test]
fn drowned_throws_tridents() {
    block_scenario::check("mob_drowned_trident");
}

/// A husk that stays under water becomes a zombie, one that leaves the water forgets, a husk hit feeds hunger.
#[test]
fn husk_becomes_zombie() {
    block_scenario::check("mob_husk_to_zombie");
}

/// Zombies that may break doors take 240 ticks for a wooden door and give up on an iron one.
#[test]
fn zombie_breaks_doors() {
    block_scenario::check("mob_zombie_break_door");
}

/// A hurt zombie on Hard calls reinforcements while its chance lasts.
#[test]
fn zombie_calls_reinforcements() {
    block_scenario::check("mob_zombie_reinforcements");
}

/// Zombies and zombified piglins charge with a spear and back off.
#[test]
fn zombie_charges_with_a_spear() {
    block_scenario::check("mob_zombie_spear");
}

/// Zombified piglins get angry at a hit player, alert their neighbours and calm down.
#[test]
fn zombified_piglin_anger() {
    block_scenario::check("mob_zombified_piglin_anger");
}

/// A baby zombie rides a chicken that is near, one that is created for it, or none, by the level random at finalizeSpawn.
#[test]
fn baby_zombie_jockeys() {
    block_scenario::check("mob_zombie_jockey");
}
