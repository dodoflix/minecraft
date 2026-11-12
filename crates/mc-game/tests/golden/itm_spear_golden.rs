//! The spear: the STAB action (`PiercingWeapon.attack`, `Player.stabAttack`), the charge of a spear in use (`KineticWeapon.damageEntities`,
//! `recentKineticEnemies`) and the lunge enchantment, replayed on the real vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level.

use crate::common;

#[test]
fn stab_hits_every_entity_along_the_view_line() {
    common::block_scenario::check("itm_spear_stab");
}

#[test]
fn charge_damages_knocks_back_and_dismounts_by_speed() {
    common::block_scenario::check("itm_spear_charge");
}

#[test]
fn lunge_enchantment_pushes_the_stabbing_player() {
    common::block_scenario::check("itm_spear_lunge_enchant");
}
