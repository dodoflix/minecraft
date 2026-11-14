//! Slimes, magma cubes and sulfur cubes against vanilla: the chase with the cube move control and touch damage, the split on death
//! with loot and experience by size, the lava boost of the magma cube, which hostile mobs count as enemies and what sulfur cubes
//! swallow, give back, ignite and follow
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

/// Sulfur cubes that stand idle with and without a block, bounce and are hit.
#[test]
fn sulfur_cube_idle() {
    common::block_scenario::check("mob_sulfur_cube_idle");
}

/// Sulfur cubes swallowing items and blocks: tempting, picking up, buckets and the pickup timer.
#[test]
fn sulfur_absorb() {
    common::block_scenario::check("mob_sulfur_absorb");
}

/// Shears taking the swallowed block back, immunities while a block is held and the knockback of a hit.
#[test]
fn sulfur_eject() {
    common::block_scenario::check("mob_sulfur_eject");
}

/// Sulfur cubes holding TNT: priming, fuse, explosion, redstone and `tnt_explodes`.
#[test]
fn sulfur_fuse() {
    common::block_scenario::check("mob_sulfur_fuse");
}

/// Sulfur cubes following their tempting items.
#[test]
fn sulfur_temptation() {
    common::block_scenario::check("mob_sulfur_temptation");
}

/// Sulfur cubes splitting on death.
#[test]
fn sulfur_split() {
    common::block_scenario::check("mob_sulfur_split");
}
