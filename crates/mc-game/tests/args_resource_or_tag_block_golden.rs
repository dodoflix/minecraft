//! `ResourceOrTagArgument` over `minecraft:block` against the vanilla golden (`fixtures/args/resource_or_tag_block_golden.txt`).
mod args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_or_tag_block_matches_vanilla() {
    args_common::replay(
        "resource_or_tag_block",
        include_str!("fixtures/args/resource_or_tag_block_golden.txt"),
        |ctx| Arc::new(ResourceOrTagArgument::resource_or_tag(ctx, "block")),
        |v, _| {
            let r = v.downcast_ref::<ResourceOrTag>().expect("result");
            let kind = match r {
                ResourceOrTag::Resource(k) => format!("resource ResourceKey[minecraft:root / {}]", k.registry),
                ResourceOrTag::Tag { key, members } => format!("tag ResourceKey[minecraft:root / {}] n={}", key.registry, members.len()),
            };
            format!("{}|{kind}", r.as_printable())
        },
    );
}
