//! The area effect cloud replayed against the real vanilla level (`scripts/block-oracle.sh NAME`): the NBT codec, the packets of
//! its pairing and its shrinking radius, who it affects and with what, wait time, radius, duration and reapplication tick by tick,
//! and the `/execute on origin` relation to its owner.

use crate::common;

#[test]
fn cloud_nbt_codec() {
    common::block_scenario::check("ent_aec_nbt");
}

#[test]
fn cloud_packets() {
    common::block_scenario::check("ent_aec_packets");
}

#[test]
fn cloud_victims() {
    common::block_scenario::check("ent_aec_victims");
}

#[test]
fn cloud_origin_is_its_living_owner() {
    common::block_scenario::check("ent_aec_origin");
}

#[test]
fn cloud_radius_duration_and_reapplication() {
    common::block_scenario::check("area_effect_cloud");
}
