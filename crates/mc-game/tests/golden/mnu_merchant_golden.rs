//! Trading with a villager through the player's packet handlers: the screen packets, the payments filling from the inventory,
//! statistics, the villager_trade advancement, experience, offers out of stock, hints, a villager levelling up and the screens of a
//! creative player and a spectator (scenario `tests/oracle/scenarios/mnu_trade_session.txt` replayed on the real vanilla level,
//! recorded with `scripts/block-oracle.sh mnu_trade_session`).

use crate::common;

#[test]
fn a_trading_session_selects_pays_and_takes_like_vanilla() {
    common::block_scenario::check("mnu_trade_session");
}
