//! `HeightmapTypeArgument` against the vanilla golden (`fixtures/args/heightmap_type_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn heightmap_type_matches_vanilla() {
    args_common::replay(
        "heightmap_type",
        include_str!("../fixtures/args/heightmap_type_golden.txt"),
        |_| Arc::new(HeightmapTypeArgument),
        |v, _src| v.downcast_ref::<mc_world::heightmap::HeightmapType>().expect("h").serialization_key().to_owned(),
    );
}
