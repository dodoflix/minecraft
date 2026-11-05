//! `instrument` / `jukebox_song` registries decoded from the vanilla data pack, goat horn use duration and the
//! `JukeboxSongPlayer` state machine (vanilla sources: `Instrument`, `InstrumentItem`, `JukeboxSong`, `JukeboxSongPlayer`).
mod common;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_data::Item;
use mc_game::registry::{RegistryAccess, keys};
use mc_game::server::level::ServerLevel;
use mc_game::world::item::{ItemStack, JukeboxSongPlayer, behaviour, get_instrument, jukebox_song_from_stack, set_item_registries};
use mc_protocol::components::{Holder, INSTRUMENT, InstrumentComponent, Instrument, JUKEBOX_PLAYABLE, JukeboxSong};
use std::path::PathBuf;
use std::sync::LazyLock;

static ACCESS: LazyLock<RegistryAccess> = LazyLock::new(|| {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (_, stem) = common::load_world(dir.path(), mc_world::level_storage::WorldDataConfiguration::DEFAULT, true).expect("default world loads");
    stem.registries.composite_access().clone()
});

fn data_dir(name: &str) -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../ref/src/data/minecraft")).join(name)
}

fn json_files(dir: &str) -> Vec<(String, serde_json::Value)> {
    let mut files: Vec<_> = std::fs::read_dir(data_dir(dir))
        .unwrap()
        .map(|e| {
            let path = e.unwrap().path();
            (path.file_stem().unwrap().to_str().unwrap().to_owned(), serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap())
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

#[test]
fn every_vanilla_instrument_is_decoded() {
    let files = json_files("instrument");
    assert_eq!(files.len(), 8);
    for (name, json) in files {
        let id = Identifier::parse(&format!("minecraft:{name}")).unwrap();
        let instrument: &Instrument = ACCESS.get(keys::INSTRUMENT, &id).unwrap_or_else(|| panic!("instrument {id} not decoded"));
        assert_eq!(f64::from(instrument.use_duration), json["use_duration"].as_f64().unwrap(), "{name}");
        assert_eq!(f64::from(instrument.range), json["range"].as_f64().unwrap(), "{name}");
        assert_eq!(instrument.durability_damage, json.get("durability_damage").and_then(|v| v.as_i64()).unwrap_or(0) as i32);
    }
}

#[test]
fn every_vanilla_jukebox_song_is_decoded_and_every_disc_resolves() {
    let files = json_files("jukebox_song");
    assert_eq!(files.len(), 22);
    for (name, json) in &files {
        let id = Identifier::parse(&format!("minecraft:{name}")).unwrap();
        let song: &JukeboxSong = ACCESS.get(keys::JUKEBOX_SONG, &id).unwrap_or_else(|| panic!("song {id} not decoded"));
        assert_eq!(f64::from(song.length_in_seconds), json["length_in_seconds"].as_f64().unwrap(), "{name}");
        assert_eq!(i64::from(song.comparator_output), json["comparator_output"].as_i64().unwrap(), "{name}");
    }
    let mut discs = 0;
    for item in Item::iter() {
        let stack = ItemStack::new(item);
        if let Some(Holder::Reference(song)) = jukebox_song_from_stack(&stack) {
            assert!(ACCESS.get::<JukeboxSong>(keys::JUKEBOX_SONG, &song).is_some(), "{item} plays unknown song {song}");
            discs += 1;
        }
    }
    assert_eq!(discs, 22, "items with a jukebox_playable component in items.json");
}

#[test]
fn goat_horn_use_duration_comes_from_its_instrument() {
    set_item_registries(ACCESS.clone());
    let mut horn = ItemStack::new(Item::GOAT_HORN);
    horn.set(INSTRUMENT, InstrumentComponent { instrument: Holder::reference("minecraft:ponder_goat_horn") });
    let instrument = get_instrument(&ACCESS, &horn).unwrap();
    assert_eq!(instrument.range, 256.0);
    assert_eq!(behaviour(Item::GOAT_HORN).get_use_duration(&horn), 140);
    // no instrument component value: zero duration (`Optional.empty().orElse(0)`)
    let plain = ItemStack::new(Item::GOAT_HORN);
    assert_eq!(get_instrument(&ACCESS, &plain).is_some(), plain.get(&INSTRUMENT).is_some());
}

#[test]
fn song_player_plays_ticks_and_stops_after_the_padded_length() {
    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    level.registries = ACCESS.clone();
    let disc = ItemStack::new(Item::MUSIC_DISC_13);
    assert!(disc.get(&JUKEBOX_PLAYABLE).is_some());
    let song = jukebox_song_from_stack(&disc).unwrap();
    let mut player = JukeboxSongPlayer::new(BlockPos::new(0, 64, 0));
    assert!(!player.is_playing());
    assert!(player.play(&mut level, song.clone()));
    assert!(player.is_playing());
    // `13`: 178 s = 3560 ticks, finished at 3560 + 20 elapsed ticks
    for tick in 0..3580 {
        assert!(!player.tick(&mut level, None), "ended early at tick {tick}");
        assert_eq!(player.get_ticks_since_song_started(), tick + 1);
    }
    assert!(player.tick(&mut level, None), "stops when finished");
    assert!(!player.is_playing());
    assert_eq!(player.get_ticks_since_song_started(), 0);
    assert!(!player.stop(&mut level, None));
    // `setSongWithoutPlaying` ignores a finished position and keeps a running one
    player.set_song_without_playing(&ACCESS, song.clone(), 3580);
    assert!(!player.is_playing());
    player.set_song_without_playing(&ACCESS, song, 100);
    assert!(player.is_playing());
    assert_eq!(player.get_ticks_since_song_started(), 100);
    assert!(player.stop(&mut level, None));
}
