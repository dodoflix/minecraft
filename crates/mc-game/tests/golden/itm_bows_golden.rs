//! Bows, crossbows and tridents used by a player (`use`, `release`, `tick`, `swap`, `select`, the dumps `use:`, `inventory:`, `stats:`, `nbt:`)
//! replayed on the real vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level.

use crate::common;

#[test]
fn bows_draw_shoot_and_use_up_arrows() {
    common::block_scenario::check("itm_bow");
}

#[test]
fn crossbows_charge_load_and_shoot() {
    common::block_scenario::check("itm_crossbow");
}

#[test]
fn tridents_are_thrown_or_ride_the_riptide() {
    common::block_scenario::check("itm_trident");
}
