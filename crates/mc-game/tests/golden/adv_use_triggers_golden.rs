//! Advancement criteria fired by using blocks and interacting with entities against the vanilla oracle (ticking-level replay, data
//! pack `tests/oracle/datapacks/adv_use`). Scenarios: `tests/oracle/scenarios/adv_*.txt`, fixtures: `tests/fixtures/entity_oracle/`
//! (`scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

/// Using blocks with and without an item: the stack each trigger sees, sneaking, adventure, spectator and creative players.
#[test]
fn using_blocks_awards_the_user() {
    check("adv_block_use");
}

/// The stack `any_block_use` sees when the use replaces the hand by a stack of the same item (a patterned banner cleaned in a cauldron).
#[test]
fn using_blocks_that_replace_the_hand_awards_the_used_object() {
    check("adv_block_use_replaced");
}

/// Interacting with entities and shearing their equipment: brush, bucket, wheat, name tag, lead, boat, saddles, wolf armor, a sulfur cube
/// and dragon breath in a bottle.
#[test]
fn interacting_with_entities_awards_the_player() {
    check("adv_entity_interaction");
}

/// Throwing an eye of ender at a located stronghold: the distance the advancement sees, for a survival and a creative player.
#[test]
fn throwing_an_eye_awards_the_thrower() {
    check("adv_ender_eye");
}

/// An eye of ender used where nothing is located awards nothing.
#[test]
fn an_eye_without_a_stronghold_awards_nothing() {
    check("adv_ender_eye_nostructure");
}

/// A dolphin takes an item floating in its pool unless its main hand is full.
#[test]
fn thrown_items_picked_up_by_a_dolphin_award_the_thrower() {
    check("adv_thrown_pickup_dolphin");
}

/// A pillager of a raid takes the ominous banner a player throws once the raid has no leader and ignores a crossbow.
#[test]
fn thrown_banner_picked_up_by_a_raider_awards_the_thrower() {
    check("adv_thrown_pickup_raid");
}

/// An allay that likes the player delivers an item above a note block the player played.
#[test]
fn allay_delivery_to_a_note_block_awards_the_liked_player() {
    check("adv_allay_note_block");
}
