//! `ResourceOrTagKeyArgument` over `minecraft:worldgen/structure` against the vanilla golden (`fixtures/args/resource_or_tag_key_structure_golden.txt`).
mod args_common;

use mc_game::commands::arguments::resource_arguments::*;
use std::sync::Arc;

#[test]
fn resource_or_tag_key_structure_matches_vanilla() {
    args_common::replay(
        "resource_or_tag_key_structure",
        include_str!("fixtures/args/resource_or_tag_key_structure_golden.txt"),
        |_| Arc::new(ResourceOrTagKeyArgument::resource_or_tag_key("worldgen/structure")),
        |v, _| {
            let r = v.downcast_ref::<ResourceOrTagKey>().expect("result");
            let kind = match r {
                ResourceOrTagKey::Resource(k) => format!("resource ResourceKey[minecraft:root / {}]", k.registry),
                ResourceOrTagKey::Tag(k) => format!("tag ResourceKey[minecraft:root / {}]", k.registry),
            };
            format!("{}|{kind}", r.as_printable())
        },
    );
}
