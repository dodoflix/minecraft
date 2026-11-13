//! The smithing table: the in-process oracle scripts `oracle/menu_scripts/smithing*.txt` (real vanilla `SmithingMenu` of a real player,
//! recorded with `scripts/menu-oracle.sh`) replayed on our menu, and the open through the player's listener
//! (`oracle/scenarios/mnu_open_smithing.txt`, `scripts/block-oracle.sh`).

use crate::common;
use crate::common::menu_oracle::replay;

/// Slot predicates, every netherite upgrade and trim pattern, results with the base's components, taking in every way, quick moves, drags,
/// closing and three random sessions.
#[test]
fn smithing_menu_script_matches_vanilla() {
    let ops = replay("menu_oracle", "smithing");
    assert!(ops > 500, "fixture has {ops} ops");
}

/// Taking a result unlocks its recipe and counts the crafted item.
#[test]
fn smithing_results_unlock_their_recipes() {
    let ops = replay("menu_oracle", "smithing_recipes");
    assert!(ops > 30, "fixture has {ops} ops");
}

#[test]
fn the_smithing_table_opens_its_menu_for_survival_and_spectator_players() {
    common::block_scenario::check("mnu_open_smithing");
}
