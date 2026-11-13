//! Replays of the block-provided menus against the REAL vanilla menus (scripts in `oracle/menu_scripts/`, recordings in
//! `fixtures/menu_oracle/`, see `common::menu_oracle`): brewing stand, beacon, lectern.

use crate::common;
use crate::common::menu_oracle::replay;

/// Slot kinds, quick move, drags, brewing and fuel data, bottle states, the stand's own ticks and eight seeded random sessions
/// (five of them with ticks in between).
#[test]
fn brewing_stand_menu_matches_vanilla() {
    let ops = replay("menu_oracle", "brewing_stand");
    assert!(ops > 2000, "fixture has {ops} ops");
}

/// Payment items, quick move, `removed`, `updateEffects` for every effect pair at pyramid levels 1-4 and a sample without a pyramid (including the
/// invalid ones and vanilla's `NullPointerException` case), the menu closing with the block.
#[test]
fn beacon_menu_matches_vanilla() {
    let ops = replay("menu_oracle", "beacon");
    assert!(ops > 1000, "fixture has {ops} ops");
}

/// Page buttons and their clamping, the page-change pulse, taking the book (`mayBuild`, full inventory), clicks on the book
/// slot (no swaps), a book with one page, a writable book.
#[test]
fn lectern_menu_matches_vanilla() {
    let ops = replay("menu_oracle", "lectern");
    assert!(ops > 80, "fixture has {ops} ops");
}

/// `ServerboundSetBeaconPacket` through the player's listener: the valid selections, the invalid ones that disconnect the player,
/// a menu that is not a beacon and a beacon that stopped being valid.
#[test]
fn set_beacon_packets_apply_or_disconnect_like_vanilla() {
    common::block_scenario::check("mnu_set_beacon");
}

/// The brewing stand menu through the player's listener and the server's tick order (block entities tick after the player): fuel,
/// progress, finished potions, the statistic and breaking the block under the menu.
#[test]
fn the_brewing_stand_menu_follows_the_stand_ticks_like_vanilla() {
    common::block_scenario::check("mnu_brewing_stand");
}

/// The lectern menu through the player's listener: page buttons, taking the book as a survival and as an adventure player, a full
/// inventory and the menu closing once the book is gone.
#[test]
fn the_lectern_menu_buttons_follow_vanilla() {
    common::block_scenario::check("mnu_lectern_take");
}
