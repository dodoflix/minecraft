//! `ScoreboardSaveData` format vs the real classes (`fixtures/ScoreboardGolden.java`).

use crate::scoreboard_build::build;
use mc_game::world::scores::Scoreboard;
use mc_game::world::scores::scoreboard_save_data::{ScoreboardSaveData, ScoreboardSaveDataPacked};
use mc_nbt::{ListTag, Tag};

/// Lists written from Java hash sets/maps have no stable order: sort every list by its SNBT.
fn canon(tag: &Tag) -> Tag {
    match tag {
        Tag::Compound(c) => {
            let mut out = mc_nbt::CompoundTag::new();
            for (k, v) in c.iter() {
                out.put(k.clone(), canon(v));
            }
            Tag::Compound(out)
        }
        Tag::List(l) => {
            let mut items: Vec<Tag> = l.iter().map(canon).collect();
            items.sort_by_key(|t| t.to_string());
            let mut out = ListTag::new();
            for i in items {
                out.push(i);
            }
            Tag::List(out)
        }
        other => other.clone(),
    }
}

fn read(name: &str) -> Tag {
    let text = std::fs::read_to_string(format!(
        "{}/../mc-game/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    mc_nbt::snbt::parse(text.trim()).unwrap()
}

#[test]
fn our_scoreboard_encodes_like_vanilla() {
    let ours = build().store_to_tag().unwrap();
    assert_eq!(canon(&ours), canon(&read("scoreboard.golden")));
    assert_eq!(canon(&ours), canon(&read("scoreboard_in.snbt")));
}

#[test]
fn vanilla_data_loads_and_round_trips() {
    let golden = read("scoreboard.golden");
    let packed = ScoreboardSaveDataPacked::decode(&golden)
        .map_err(|e| e.error)
        .unwrap();
    let mut sb = Scoreboard::new_server();
    sb.load(&packed);
    assert_eq!(canon(&sb.store_to_tag().unwrap()), canon(&golden));
    assert_eq!(sb.get_players_team("Bob").unwrap().get_name(), "red_team");
    assert!(
        sb.is_dirty(),
        "loading dirties a server scoreboard like vanilla"
    );
    // vanilla's own load -> store of our file
    assert_eq!(
        canon(&read("scoreboard_roundtrip.golden")),
        canon(&read("scoreboard_in.snbt"))
    );
}

#[test]
fn save_data_set_data_dirty_semantics() {
    use mc_game::world::level::saveddata::SavedData;
    let mut sd = ScoreboardSaveData::default();
    sd.set_data(ScoreboardSaveDataPacked::default());
    assert!(!sd.is_dirty(), "equal data does not dirty");
    let mut sb = Scoreboard::new_server();
    sb.add_player_team("t");
    sb.store_to_save_data_if_dirty(&mut sd);
    assert!(sd.is_dirty());
    assert!(!sb.is_dirty());
}
