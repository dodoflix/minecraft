//! `ScoreboardSaveData` format vs the real classes (`fixtures/ScoreboardGolden.java`).

use mc_game::world::scores::scoreboard_save_data::{ScoreboardSaveData, ScoreboardSaveDataPacked};
use mc_game::world::scores::score::for_name_only;
use mc_game::world::scores::{CollisionRule, DisplaySlot, NumberFormat, ObjectiveCriteria, RenderType, Scoreboard, TeamColor, Visibility};
use mc_nbt::{ListTag, Tag};
use mc_text::{ChatFormatting, Component, Style, TranslatableArg};

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

fn build() -> Scoreboard {
    let mut b = Scoreboard::new();
    b.add_objective("kills", ObjectiveCriteria::DUMMY, Component::literal("Kills"), RenderType::Integer, false, None).unwrap();
    b.add_objective("hp", ObjectiveCriteria::HEALTH, Component::literal("HP").with_format(ChatFormatting::Red), RenderType::Hearts, true, Some(NumberFormat::Blank)).unwrap();
    b.add_objective(
        "trig",
        ObjectiveCriteria::TRIGGER,
        Component::translatable("some.key", vec![TranslatableArg::String("arg".into())]),
        RenderType::Integer,
        true,
        Some(NumberFormat::Fixed(Component::literal("fixed"))),
    )
    .unwrap();
    b.add_objective(
        "tk",
        ObjectiveCriteria::team_kill(TeamColor::Red),
        Component::literal("TK"),
        RenderType::Integer,
        false,
        Some(NumberFormat::Styled(Style::EMPTY.with_color_formatting(Some(ChatFormatting::Gold)).with_bold(Some(true)))),
    )
    .unwrap();
    let alice = for_name_only("Alice");
    let bob = for_name_only("Bob");
    b.get_or_create_player_score(&alice, "kills").unwrap().set(5).unwrap();
    {
        let mut s = b.get_or_create_player_score(&bob, "kills").unwrap();
        s.set(-3).unwrap();
        s.unlock();
        s.set_display(Some(Component::literal("Bobby")));
        s.number_format_override(Some(NumberFormat::Fixed(Component::literal("x"))));
    }
    {
        let mut s = b.get_or_create_player_score(&alice, "trig").unwrap();
        s.set(0).unwrap();
        s.unlock();
    }
    b.get_or_create_player_score_forced(&for_name_only("#hidden"), "tk", true).unwrap().set(i32::MAX).unwrap();
    b.set_display_objective(DisplaySlot::Sidebar, Some("kills"));
    b.set_display_objective(DisplaySlot::List, Some("hp"));
    b.set_display_objective(DisplaySlot::TeamRed, Some("tk"));
    b.add_player_team("red_team");
    {
        let mut t = b.team_mut("red_team").unwrap();
        t.set_display_name(Component::literal("Reds"));
        t.set_color(Some(TeamColor::Red));
        t.set_allow_friendly_fire(false);
        t.set_see_friendly_invisibles(false);
        t.set_player_prefix(Some(Component::literal("[R] ").with_format(ChatFormatting::Bold)));
        t.set_player_suffix(Some(Component::literal("!")));
        t.set_name_tag_visibility(Visibility::HideForOtherTeams);
        t.set_death_message_visibility(Visibility::Never);
        t.set_collision_rule(CollisionRule::PushOwnTeam);
    }
    b.add_player_to_team("Alice", "red_team");
    b.add_player_to_team("Bob", "red_team");
    b.add_player_team("plain");
    b.add_player_to_team("Carol", "plain");
    b
}

fn read(name: &str) -> Tag {
    let text = std::fs::read_to_string(format!("{}/../mc-game/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap();
    mc_nbt::snbt::parse(text.trim()).unwrap()
}

#[test]
fn our_scoreboard_encodes_like_vanilla() {
    let ours = build().store_to_tag().unwrap();
    assert_eq!(canon(&ours), canon(&read("scoreboard.golden")));
    // the file the Java side re-reads; `MC_UPDATE_FIXTURES=1` rewrites it (then rerun ScoreboardGolden.java)
    if std::env::var_os("MC_UPDATE_FIXTURES").is_some() {
        std::fs::write(format!("{}/../mc-game/tests/fixtures/scoreboard_in.snbt", env!("CARGO_MANIFEST_DIR")), format!("{ours}\n")).unwrap();
    }
    assert_eq!(canon(&ours), canon(&read("scoreboard_in.snbt")));
}

#[test]
fn vanilla_data_loads_and_round_trips() {
    let golden = read("scoreboard.golden");
    let packed = ScoreboardSaveDataPacked::decode(&golden).map_err(|e| e.error).unwrap();
    let mut sb = Scoreboard::new_server();
    sb.load(&packed);
    assert_eq!(canon(&sb.store_to_tag().unwrap()), canon(&golden));
    assert_eq!(sb.get_players_team("Bob").unwrap().get_name(), "red_team");
    assert!(sb.is_dirty(), "loading dirties a server scoreboard like vanilla");
    // vanilla's own load -> store of our file
    assert_eq!(canon(&read("scoreboard_roundtrip.golden")), canon(&read("scoreboard_in.snbt")));
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
