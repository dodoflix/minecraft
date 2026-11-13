//! The inventory screen of tamed mounts: the menu replayed against the real vanilla menu (`oracle/menu_scripts/mount.txt`, recorded with
//! `scripts/menu-oracle.sh mount`) and horses, donkeys, llamas and a mule through the player's packet handlers (scenario
//! `oracle/scenarios/mnu_horse_inventory.txt`, recorded with `scripts/block-oracle.sh mnu_horse_inventory`).

use crate::common;
use crate::common::menu_oracle::replay;

/// Saddle, armor and chest slots, shift-click both ways, drags, the equipment a mount keeps, the screen closing when the mount is out of
/// reach, has a new inventory, dies or leaves, and seeded random sessions on every kind of mount.
#[test]
fn mount_menu_script_matches_vanilla() {
    let ops = replay("menu_oracle", "mount");
    assert!(ops > 1000, "fixture has {ops} ops");
}

/// Opening by sneaking or from the saddle, what each slot takes, shift-click, the screen packets with the mount's entity id, a spectator,
/// an untamed horse and the screen closing when the mount is out of reach, replaced its chest or is gone.
#[test]
fn mount_inventories_open_and_move_items_like_vanilla() {
    common::block_scenario::check("mnu_horse_inventory");
}
