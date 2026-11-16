//! Travel into and out of the End in the multi-level replay: `tests/oracle/scenarios/portal_end_*.txt`, `end_gateway_*.txt`, fixtures from
//! `scripts/block-oracle.sh NAME` (the real vanilla server with its overworld and End).
use crate::common::block_scenario::check;

/// An end portal block sends players, items and mobs to the End platform, which is built (and cleared with drops) on arrival.
#[test]
fn end_portal_leads_to_the_platform() {
    check("portal_end_enter");
}

/// Leaving the End before the credits were seen: the player is removed from the End and told it won, and the respawn request brings it back with everything.
#[test]
fn end_exit_shows_the_credits_once() {
    check("portal_end_leave_credits");
}

/// Leaving the End after the credits: bed, respawn anchor, forced point, a missing respawn block and no point at all; non-players use the world spawn.
#[test]
fn end_exit_follows_the_respawn_point() {
    check("portal_end_leave_seen");
}

/// Experience orbs and items on the exit portal go to the overworld.
#[test]
fn orbs_and_items_leave_the_end() {
    check("portal_end_orbs");
}

/// A boat with a rider that has not seen the credits cannot leave the End; with the credits seen it takes its rider away. Entering the End they arrive together.
#[test]
fn vehicles_travel_through_the_end_portal_with_their_riders() {
    check("portal_end_vehicle");
}

/// A gateway block without an exit finds or builds one: the walk over empty chunks, the island, the exit gateway and the stored position.
#[test]
fn unlinked_gateway_creates_its_exit() {
    check("end_gateway_exit_create");
}

/// Exact and searched exits, rotation kept, items and mobs, the cooldown that holds a second traveller back.
#[test]
fn gateway_exits_are_exact_or_searched() {
    check("end_gateway_exact");
}

/// A player walks through a gateway with its motion and rotation; a second one passes while the gateway cools down.
#[test]
fn walking_players_use_gateways() {
    check("end_gateway_player");
}

/// An ender pearl thrown through a gateway arrives with no motion and lands where its owner follows.
#[test]
fn pearls_fly_through_gateways() {
    check("end_gateway_pearl");
}

/// Several entities in one gateway: one per cooldown, a pig and an arrow wait their turn.
#[test]
fn entities_wait_for_the_gateway_cooldown() {
    check("end_gateway_cooldown_entities");
}

/// Falling blocks that change level to or from the End also land in the level they left.
#[test]
fn falling_blocks_duplicate_through_the_end_portal() {
    check("falling_block_end_duplicate");
}

/// Leaving the End with and without seen credits through the respawn machinery: the win-game removal, the respawn that keeps everything, a portal to a bed.
#[test]
fn respawn_after_the_end() {
    check("respawn_end_exit");
}
