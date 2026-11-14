//! The nautilus and the zombie nautilus against vanilla: taming, saddle and armor, the inventory screen, the dash of a rider,
//! breeding, the variant by biome, the drowned jockey and the brain (`tests/oracle/scenarios/mob_*nautilus*.txt`,
//! `mnu_nautilus_inventory.txt`; fixtures regenerated with `scripts/block-oracle.sh NAME`).

use crate::common::block_scenario::check;

#[test]
fn taming_with_pufferfish_and_buckets() {
    check("mob_nautilus_tame");
}

#[test]
fn saddle_and_armor_by_hand_and_dispenser() {
    check("mob_nautilus_armor_saddle");
}

#[test]
fn inventory_screen() {
    check("mnu_nautilus_inventory");
}

#[test]
fn rider_dash_and_breath() {
    check("mob_nautilus_dash");
}

#[test]
fn breeding_tamed_babies() {
    check("mob_nautilus_breed");
}

#[test]
fn zombie_nautilus_variant_by_biome() {
    check("mob_zombie_nautilus_variant");
}

#[test]
fn drowned_jockey_on_zombie_nautilus() {
    check("mob_drowned_nautilus_jockey");
}

#[test]
fn zombie_nautilus_brain_and_sun() {
    check("mob_zombie_nautilus_ai");
}
