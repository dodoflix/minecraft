//! The marker and interaction entities replayed against the real vanilla level (`scripts/block-oracle.sh NAME`): the
//! packets of their pairing, the attack and use an interaction records and the `/execute on` relations that read them,
//! the size changes, the NBT codec, and what a marker and an interaction do (not) do to blocks and projectiles.

use crate::common;

#[test]
fn interaction_records_attack_and_use_and_resizes() {
    common::block_scenario::check("ent_interaction");
}

#[test]
fn interaction_nbt_codec() {
    common::block_scenario::check("ent_interaction_nbt");
}

#[test]
fn interaction_against_spear_projectile_plate_and_piston() {
    common::block_scenario::check("ent_interaction_world");
}

#[test]
fn marker_is_never_tracked_ticked_hurt_or_ridden() {
    common::block_scenario::check("ent_marker");
}
