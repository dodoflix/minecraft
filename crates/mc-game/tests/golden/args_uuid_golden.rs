//! `UuidArgument` against the vanilla golden (`fixtures/args/uuid_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn uuid_matches_vanilla() {
    args_common::replay(
        "uuid",
        include_str!("../fixtures/args/uuid_golden.txt"),
        |_| Arc::new(UuidArgument::uuid()),
        |v, _src| v.downcast_ref::<uuid::Uuid>().expect("u").to_string(),
    );
}
