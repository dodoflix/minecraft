//! Slimes and magma cubes against vanilla: the chase with the cube move control and touch damage, the split on death with loot and
//! experience by size, the lava boost of the magma cube and which hostile mobs count as enemies
//! (`tests/oracle/scenarios/mob_*.txt`; fixtures from `scripts/block-oracle.sh NAME`).

use crate::common;

/// Slimes of several sizes walking up to a survival player and hitting it, and the height limit of their targeting.
#[test]
fn slime_chase() {
    common::block_scenario::check("mob_slime_chase");
}

/// Slimes and magma cubes dying: children, experience, loot, teams.
#[test]
fn slime_split() {
    common::block_scenario::check("mob_slime_split");
}

/// Magma cubes chasing a player and floating in lava.
#[test]
fn magma_cube_chase() {
    common::block_scenario::check("mob_magma_cube_chase");
}

/// Golems targeting the hostile non-monsters, and leads refused by them.
#[test]
fn cube_is_enemy() {
    common::block_scenario::check("mob_cube_is_enemy");
}

/// Natural slime spawning: swamp surface by moon phase, plains, slime chunks and peaceful difficulty.
#[test]
fn slime_spawn_rules() {
    common::block_scenario::check("mob_slime_spawn_rules");
}

/// Natural magma cube spawning in the basalt deltas, and the peaceful refusal.
#[test]
fn magma_cube_spawn() {
    common::block_scenario::check("mob_magma_cube_spawn");
}
