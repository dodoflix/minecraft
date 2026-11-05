//! Differential test: `server.properties` written by the real vanilla server (`--initSettings`) vs ours.
//! Fixtures: `tests/fixtures/dedicated/{default_out,edited_in,edited_out}.properties` (produced by the vanilla server
//! with `--initSettings`).

extern crate mc_game_core as mc_game;
use mc_game::server::dedicated::java_properties::Properties;
use mc_game::server::dedicated::properties::DedicatedServerProperties;

/// Drop the date comment and the randomly generated management secret.
fn normalize(text: &str) -> Vec<String> {
    text.lines()
        .enumerate()
        .filter(|(i, _)| *i != 1)
        .map(|(_, l)| if l.starts_with("management-server-secret=") && l.len() == 25 + 40 { "management-server-secret=<random>".to_owned() } else { l.to_owned() })
        .collect()
}

fn ours(input: &str) -> String {
    let props = DedicatedServerProperties::new(Properties::load(input).unwrap()).unwrap();
    props.properties().store(Some("Minecraft server properties"))
}

#[test]
fn default_file_matches_vanilla() {
    let expected = include_str!("../../mc-game/tests/fixtures/dedicated/default_out.properties");
    assert_eq!(normalize(&ours("")), normalize(expected));
}

#[test]
fn edited_file_is_rewritten_like_vanilla() {
    let input = include_str!("../../mc-game/tests/fixtures/dedicated/edited_in.properties");
    let expected = include_str!("../../mc-game/tests/fixtures/dedicated/edited_out.properties");
    assert_eq!(normalize(&ours(input)), normalize(expected));
}
