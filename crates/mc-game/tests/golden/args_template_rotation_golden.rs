//! `TemplateRotationArgument` against the vanilla golden (`fixtures/args/template_rotation_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn template_rotation_matches_vanilla() {
    args_common::replay(
        "template_rotation",
        include_str!("../fixtures/args/template_rotation_golden.txt"),
        |_| Arc::new(TemplateRotationArgument),
        |v, _src| v.downcast_ref::<mc_core::rotation::Rotation>().expect("r").get_serialized_name().to_owned(),
    );
}
