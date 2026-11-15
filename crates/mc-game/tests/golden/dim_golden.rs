//! Golden tests of the replay with several levels (`levels overworld,the_nether,the_end`): `tests/oracle/scenarios/dim_*.txt`, fixtures from
//! `scripts/block-oracle.sh NAME` (the real vanilla server with all of its levels).
use crate::common::block_scenario::check;

/// The three levels tick together; `in DIM` and `execute in` act on one of them; a player and a mob change level by `tp_dim` and `/tp`.
#[test]
fn three_levels_tick_and_exchange_a_player_and_a_mob() {
    check("dim_smoke");
}
