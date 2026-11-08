//! Piston scenarios replayed against vanilla fixtures (`scripts/block-oracle.sh blk_pistons_*`).

use crate::common;

macro_rules! scenario {
    ($($name:ident),*) => {$(
        #[test]
        fn $name() {
            common::block_scenario::check(concat!("blk_pistons_", stringify!($name)));
        }
    )*};
}

scenario!(
    bud,
    extend_dirs,
    sticky_dirs,
    slime_honey,
    machines,
    push_limit,
    reactions,
    zero_tick,
    player_break
);
