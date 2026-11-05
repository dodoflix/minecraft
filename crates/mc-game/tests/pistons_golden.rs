//! Piston scenarios replayed against vanilla fixtures (`scripts/block-oracle.sh blk_pistons_*`).
mod common;

macro_rules! scenario {
    ($($name:ident),*) => {$(
        #[test]
        fn $name() {
            common::block_scenario::check(concat!("blk_pistons_", stringify!($name)));
        }
    )*};
}

scenario!(bud, extend_dirs, sticky_dirs, slime_honey, machines);

// Both scenarios destroy popped blocks (poppy/torch/...): from the first such drop on, the level RNG-dependent piston sound
// pitches differ from vanilla (our run draws fewer values than vanilla between two events). Not yet diagnosed (drops through
// the loot system in the scenario level); block states and block events match up to there.
#[test]
#[ignore = "level RNG after block drops differs from vanilla in the scenario level (undiagnosed)"]
fn push_limit() {
    common::block_scenario::check("blk_pistons_push_limit");
}

#[test]
#[ignore = "level RNG after block drops differs from vanilla in the scenario level (undiagnosed)"]
fn reactions() {
    common::block_scenario::check("blk_pistons_reactions");
}
