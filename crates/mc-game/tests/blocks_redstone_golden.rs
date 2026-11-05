//! Redstone wire (default + experimental evaluator), torches (incl. burnout), lamps, ores vs the real vanilla level.
mod common;

macro_rules! scenario {
    ($($name:ident),*) => {$(
        #[test]
        fn $name() {
            common::block_scenario::check(concat!("blk_redstone_", stringify!($name)));
        }
    )*};
}

scenario!(wire_line, wire_terrain, lamp, torch_clock, torch_misc, ore, exp_line, exp_terrain, exp_clock);
