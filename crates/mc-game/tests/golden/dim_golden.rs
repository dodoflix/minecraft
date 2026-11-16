//! Golden tests of the replay with several levels (`levels overworld,the_nether,the_end`): `tests/oracle/scenarios/dim_*.txt`, fixtures from
//! `scripts/block-oracle.sh NAME` (the real vanilla server with all of its levels).
use crate::common::block_scenario::check;

/// The three levels tick together; `in DIM` and `execute in` act on one of them; a player and a mob change level by `tp_dim` and `/tp`.
#[test]
fn three_levels_tick_and_exchange_a_player_and_a_mob() {
    check("dim_smoke");
}

/// `execute in`, `execute if dimension`, `@e`, `@s` and `execute as .. at @s` across the levels of a server.
#[test]
fn execute_and_selectors_reach_every_level() {
    check("dim_execute");
}

/// `clone` between levels, `forceload` per level, `spreadplayers` in the nether and `compute` with a source in another level.
#[test]
fn clone_forceload_spreadplayers_and_compute_name_their_level() {
    check("dim_commands");
}

/// A custom boss bar follows its players across dimension changes and a respawn.
#[test]
fn boss_bar_follows_its_players_across_levels() {
    check("dim_bossbar");
}

/// A dimension change with an open menu and a stack on the cursor.
#[test]
fn dimension_change_with_an_open_menu() {
    check("dim_menu");
}

/// `/forceload` per level and the chunks a player's arrival loads.
#[test]
fn chunk_tickets_belong_to_their_level() {
    check("dim_tickets");
}

/// The login packet of a join into the overworld and into the nether.
#[test]
fn login_packet_lists_the_levels_of_the_server() {
    check("dim_joinburst");
}

/// Chunks that lose their last ticket unload two ticks later; a chunk a block read touched in the tick of the removal keeps its neighbours one tick longer.
#[test]
fn unloading_follows_the_ticket_removal() {
    check("dim_unload");
}

/// `allow_entering_nether_using_portals` on and off: the arrival in the nether and the portal built there.
#[test]
fn portal_arrival_follows_the_game_rule() {
    check("dim_rules");
}

/// Players in different levels: what a level broadcasts stays in the level, what the player list broadcasts reaches every level.
#[test]
fn players_in_different_levels_hear_their_own_level() {
    check("dim_players");
}

/// `/kill` of a living entity of another level hurts it through the command's level: nobody who watches the victim hears the damage event.
#[test]
fn kill_from_another_level_broadcasts_no_damage_event() {
    check("dim_kill_other_level");
}
