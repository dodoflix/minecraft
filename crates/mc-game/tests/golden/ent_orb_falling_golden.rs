//! Experience orbs and falling blocks in the block replay: the orb sound source and lava burn, orb NBT, pickup packets, mending slots,
//! falling block block entity data, landing packets, anvil damage and the moving piston
//! (`tests/oracle/scenarios/ent_{orb,falling}_*.txt`; fixtures regenerated with `scripts/block-oracle.sh NAME`).

use crate::common;

macro_rules! golden {
    ($($test:ident => $scenario:literal),+ $(,)?) => {$(
        #[test]
        fn $test() {
            common::block_scenario::check($scenario);
        }
    )+};
}

golden!(
    falling_anvil_chain => "ent_falling_anvil_chain",
    falling_packets => "ent_falling_packets",
    falling_piston => "ent_falling_piston",
    falling_tile_entity_data => "ent_falling_tiledata",
    orb_lava_sound => "ent_orb_lava_sound",
    orb_merge_packets => "ent_orb_merge_packets",
    orb_mending_slots => "ent_orb_mending_slots",
    orb_nbt => "ent_orb_nbt",
    orb_pickup_packets => "ent_orb_pickup_packets",
);
