//! Behaviour of the legacy fixes that a recording cannot pin down.

use crate::compare::{Request, Status, run_ours};
use mc_datafix::data_fixer;
use mc_nbt::Tag;
use std::collections::BTreeSet;

/// `EntityZombieVillagerTypeFix` draws a profession from a thread-local random source when the zombie has none, so the
/// recording of vanilla cannot be compared byte for byte: the type has to be one of the six professions and the draw has
/// to vary.
#[test]
fn a_zombie_villager_without_a_profession_gets_a_random_zombie_type() {
    let request = Request {
        id: "zombie".into(),
        ty: "ref:entity".into(),
        from: 501,
        to: 502,
        input: mc_nbt::snbt::parse_compound(r#"{id:"Zombie",IsVillager:1b,VillagerProfession:9}"#)
            .expect("SNBT"),
    };
    let types: BTreeSet<i32> = (0..200)
        .map(|_| match run_ours(data_fixer(), &request).status {
            Status::Ok { tag, .. } => {
                let Some(Tag::Int(t)) = tag.get("ZombieType") else {
                    panic!("ZombieType is an int")
                };
                *t
            }
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert!(types.iter().all(|t| (0..6).contains(t)), "{types:?}");
    assert!(types.len() > 1, "the draw does not vary: {types:?}");
}
