//! Brewing stand against the real vanilla server (`tests/oracle/scenarios/brewing_cross.txt`, produced by
//! `tests/oracle/gen_brewing_cross.py`, vanilla output in `tests/fixtures/entity_oracle/brewing_cross.txt`): one stand per
//! (reagent, three containers) of the full reagent x container x potion cross product, loaded from the same NBT the
//! scenario merges into the vanilla block entity, ticked 1 + 199 + 201 times, `data get` compared after each stage.

use crate::common;

use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_game::server::level::ServerLevel;
use mc_game::world::block_entity::{BlockEntity, BrewingStandBlockEntity};
use mc_game::world::item::crafting::RecipeManager;
use mc_nbt::{CompoundTag, Tag};
use mc_world::level_storage::WorldDataConfiguration;

/// Order independent text of a tag (compound keys sorted), numeric types preserved.
fn canon(tag: &Tag) -> String {
    match tag {
        Tag::Compound(c) => {
            let mut entries: Vec<(&String, &Tag)> = c.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            format!(
                "{{{}}}",
                entries
                    .iter()
                    .map(|(k, v)| format!("{k}:{}", canon(v)))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Tag::List(l) => format!("[{}]", l.iter().map(canon).collect::<Vec<_>>().join(",")),
        other => format!("{other:?}"),
    }
}

fn compound(tag: Tag) -> CompoundTag {
    match tag {
        Tag::Compound(c) => c,
        other => panic!("not a compound: {other:?}"),
    }
}

/// `C x, y, z has the following block data: {...}` -> (x, z, snbt)
fn parse_data_get(line: &str) -> (i32, i32, CompoundTag) {
    let rest = line.strip_prefix("C ").expect("C line");
    let mut parts = rest.splitn(4, ", ");
    let x: i32 = parts.next().unwrap().parse().unwrap();
    let _y = parts.next().unwrap();
    let tail = parts.next().unwrap();
    let (z, _) = tail.split_once(' ').unwrap();
    let snbt = rest.split_once("block data: ").expect("block data").1;
    (
        x,
        z.parse().unwrap(),
        compound(mc_nbt::snbt::parse(snbt).unwrap_or_else(|e| panic!("snbt {snbt}: {e:?}"))),
    )
}

#[test]
fn brewing_stands_match_vanilla() {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (_, stem) = common::load_world(
        &dir.path().join("world"),
        WorldDataConfiguration::DEFAULT,
        true,
    )
    .expect("world loads");
    let manager = stem
        .data_pack_resources
        .listener::<RecipeManager>()
        .expect("recipe manager")
        .clone();
    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    level.registries = stem.registries.composite_access().clone();
    level.shared.set_recipe_manager(manager);

    let scenario = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/oracle/scenarios/brewing_cross.txt"
    ))
    .unwrap();
    let fixture = mc_testkit::read_gz(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/entity_oracle/brewing_cross.txt.gz"
    ));

    // stands in scenario order
    let mut stands: Vec<(i32, i32, BrewingStandBlockEntity)> = Vec::new();
    for line in scenario.lines() {
        if let Some(rest) = line.strip_prefix("cmd data merge block ") {
            let mut it = rest.splitn(4, ' ');
            let x: i32 = it.next().unwrap().parse().unwrap();
            let _y = it.next();
            let z: i32 = it.next().unwrap().parse().unwrap();
            let nbt = compound(mc_nbt::snbt::parse(it.next().unwrap()).expect("scenario snbt"));
            let state = Block::BREWING_STAND.default_state();
            let mut be = BrewingStandBlockEntity::new(BlockPos::new(x, -60, z), state);
            // `data merge` = saveWithFullMetadata, merge, loadWithComponents
            let mut full = be.save_with_full_metadata();
            for (k, v) in nbt.iter() {
                full.put(k, v.clone());
            }
            be.load_with_components(&full);
            stands.push((x, z, be));
        }
    }
    assert!(stands.len() > 900, "{} stands", stands.len());

    // the three tick stages of the scenario: `tick 1`, `tick 199`, `tick 201`
    let mut expected = fixture
        .lines()
        .filter(|l| l.starts_with("C ") && l.contains("block data:"))
        .map(parse_data_get);
    let mut mismatches = Vec::new();
    let mut compared = 0usize;
    for (stage, ticks) in [(0usize, 1), (1, 199), (2, 201)] {
        for _ in 0..ticks {
            for (_, _, be) in stands.iter_mut() {
                be.server_tick(&mut level);
            }
        }
        // stage 0/1 print every 9th stand, the last stage all of them
        let indices: Vec<usize> = if stage < 2 {
            (0..stands.len()).step_by(9).collect()
        } else {
            (0..stands.len()).collect()
        };
        for i in indices {
            let (x, z, want) = expected.next().expect("fixture has the data get");
            let (sx, sz, be) = &stands[i];
            assert_eq!((x, z), (*sx, *sz), "fixture order");
            let ours = be.save_with_full_metadata();
            compared += 1;
            let (a, b) = (canon(&Tag::Compound(ours)), canon(&Tag::Compound(want)));
            if a != b {
                mismatches.push(format!(
                    "stage {stage} stand {i} ({x},{z})\n ours:    {a}\n vanilla: {b}"
                ));
            }
        }
    }
    assert!(
        expected.next().is_none(),
        "fixture has more data gets than compared"
    );
    assert!(
        mismatches.is_empty(),
        "{} of {compared} stands differ, first:\n{}",
        mismatches.len(),
        mismatches
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
