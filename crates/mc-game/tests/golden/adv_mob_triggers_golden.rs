//! Advancement criteria fired by mobs against the vanilla oracle (ticking-level replay): breeding, taming, summoning golems and the
//! wither, villager trades and raids. Scenarios: `tests/oracle/scenarios/adv_*.txt`, fixtures: `tests/fixtures/entity_oracle/`
//! (`scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

/// One pair of every animal of the bred-all-animals advancement, love causes taken from either parent or missing, eggs instead of
/// babies, and natural breeding of cows, pigs and turtles with two players.
#[test]
fn animals_bred_award_the_love_cause() {
    check("adv_bred_animals");
}

/// Cats and wolves of every variant, mounts tamed by name, and the food paths of a wolf, a cat and a parrot.
#[test]
fn tamed_animals_award_the_owner() {
    check("adv_tamed_animals");
}

/// Snow and iron golems built with a carved pumpkin, and a wither built with skulls: who is awarded the summon, and nobody when the
/// pumpkin is set by a command.
#[test]
fn summoned_golems_and_withers_award_players() {
    check("adv_summoned_golems");
}

/// The respawn ritual's end awards the summoned dragon to the players of the fight's boss bar only.
#[test]
fn summoned_dragon_awards_the_boss_bar_players() {
    check("adv_dragon_respawn");
}

/// Winning a raid makes the fighting player a hero of the village, killing the banner captain awards the exile advancement.
#[test]
fn raid_victory_awards_the_hero() {
    check("adv_raid_hero");
}

/// Taking a trade result awards the trade advancements for villagers and wandering traders, at ground level and at the build limit.
#[test]
fn trades_award_the_trader() {
    check("adv_villager_trades");
}
