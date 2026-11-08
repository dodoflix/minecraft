//! Golden tests of doors, trapdoors, fence gates and the connecting blocks against the real vanilla level.
//! Scenarios: `tests/oracle/scenarios/blk_dc_np_*.txt` (setBlock-driven: redstone, updateShape chains, support removal;
//! no players needed), fixtures from `scripts/block-oracle.sh NAME`.
use crate::common::block_scenario::check;

#[test]
fn doors_redstone_support_and_types() {
    check("blk_dc_np_doors");
}

#[test]
fn trapdoors_waterlogged_redstone_types() {
    check("blk_dc_np_trapdoors");
}

#[test]
fn fence_gates_in_wall_and_redstone() {
    check("blk_dc_np_fence_gates");
}

#[test]
fn fences_bars_walls_gates_random_updates() {
    check("blk_dc_np_fences");
}

#[test]
fn walls_tall_low_post_random_updates() {
    check("blk_dc_np_walls");
}

#[test]
fn stairs_shapes_random_updates() {
    check("blk_dc_np_stairs");
}

#[test]
fn ladders_chains_lanterns_carpets_random_updates() {
    check("blk_dc_np_misc");
}

#[test]
fn trapdoors_waterlogged_water_ticks() {
    check("blk_dc_np_trapdoors_wet");
}

#[test]
fn fences_bars_walls_gates_waterlogged_random_updates() {
    check("blk_dc_np_fences_wet");
}

#[test]
fn stairs_waterlogged_random_updates() {
    check("blk_dc_np_stairs_wet");
}

#[test]
fn ladders_chains_lanterns_waterlogged_random_updates() {
    check("blk_dc_np_misc_wet");
}

/// Player-driven scenarios (survival/creative players placing and using the blocks).
macro_rules! player_golden {
    ($($name:ident => $scenario:literal),+ $(,)?) => {$(
        #[test]
        fn $name() {
            check($scenario);
        }
    )+};
}

player_golden!(
    doors_player_placement_and_use => "blk_dc_doors",
    connecting_blocks_player_placement => "blk_dc_connect",
    fence_gates_player_placement_and_use => "blk_dc_fence_gates",
    misc_shaped_blocks_player_placement => "blk_dc_misc",
    stairs_slabs_player_placement => "blk_dc_stairs_slabs",
    trapdoors_player_placement_and_use => "blk_dc_trapdoors",
);
