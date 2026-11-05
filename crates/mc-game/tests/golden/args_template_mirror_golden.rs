//! `TemplateMirrorArgument` against the vanilla golden (`fixtures/args/template_mirror_golden.txt`, `scripts/args-golden.sh`).

use crate::args_common;

use mc_game::commands::arguments::*;
use std::sync::Arc;

#[test]
fn template_mirror_matches_vanilla() {
    args_common::replay(
        "template_mirror",
        include_str!("../fixtures/args/template_mirror_golden.txt"),
        |_| Arc::new(TemplateMirrorArgument),
        |v, _src| v.downcast_ref::<mc_core::mirror::Mirror>().expect("m").get_serialized_name().to_owned(),
    );
}
