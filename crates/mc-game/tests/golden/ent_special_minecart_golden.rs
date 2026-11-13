//! The furnace, TNT, command block and spawner minecarts against vanilla: fuel and pushing, ignition and explosions, the command run by an
//! activator rail and edited through the packet, the spawner timer (`tests/oracle/scenarios/mc_old_{furnace,tnt_collide}.txt`,
//! `ent_{furnace,tnt,command,spawner}_minecart*.txt`; fixtures regenerated with `scripts/block-oracle.sh NAME`).

use crate::common;

#[test]
fn furnace_cart_pushes_along_the_rails_and_burns_out() {
    common::block_scenario::check("mc_old_furnace");
}

#[test]
fn furnace_cart_takes_fuel_from_players() {
    common::block_scenario::check("ent_furnace_minecart_fuel");
}

#[test]
fn tnt_cart_explodes_against_a_wall_and_from_fire() {
    common::block_scenario::check("mc_old_tnt_collide");
}

#[test]
fn tnt_cart_ignition() {
    common::block_scenario::check("ent_tnt_minecart_ignite");
}

#[test]
fn command_block_cart_runs_its_command_and_takes_the_editor_packet() {
    common::block_scenario::check("ent_command_minecart");
}

#[test]
fn spawner_cart_spawns_like_a_spawner_block() {
    common::block_scenario::check("ent_spawner_minecart");
}
