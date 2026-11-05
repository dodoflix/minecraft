//! Codecs of the saved-data types against the real classes (`fixtures/SavedDataGolden.java` -> `saved_data.golden`).

extern crate mc_game_core as mc_game;
use std::collections::HashMap;

use mc_core::ident::Identifier;
use mc_game::world::clock::ClockState;
use mc_game::world::level::saveddata::random_sequences::RandomSequences;
use mc_game::world::level::saveddata::wandering_trader_data::WanderingTraderData;
use mc_game::world::level::saveddata::weather_data::WeatherData;
use mc_game::util::timer_queue::{FunctionCallback, FunctionHost, FunctionTagCallback, TimerCallbacks, TimerQueue};
use mc_nbt::Tag;

fn golden() -> HashMap<String, String> {
    include_str!("../../mc-game/tests/fixtures/saved_data.golden").lines().map(|l| l.split_once('\t').unwrap()).map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

fn snbt(g: &HashMap<String, String>, k: &str) -> Tag {
    mc_nbt::snbt::parse(&g[k]).unwrap_or_else(|e| panic!("{k}: {e:?}"))
}

fn id(s: &str) -> Identifier {
    Identifier::parse(s).unwrap()
}

#[test]
fn weather_and_wandering_trader() {
    let g = golden();
    assert_eq!(WeatherData::new(1, 2, 3, true, false).encode().unwrap(), snbt(&g, "weather"));
    assert_eq!(WeatherData::decode(&snbt(&g, "weather")).map_err(|e| e.error).unwrap().get_rain_time(), 2);
    assert_eq!(WanderingTraderData::default().encode().unwrap(), snbt(&g, "wandering_default"));
    assert_eq!(WanderingTraderData::new(1, 25).encode().unwrap(), snbt(&g, "wandering_delay"));
    assert_eq!(WanderingTraderData::new(24000, 3).encode().unwrap(), snbt(&g, "wandering_chance"));
    let back = WanderingTraderData::decode(&snbt(&g, "wandering_chance")).map_err(|e| e.error).unwrap();
    assert_eq!((back.spawn_delay(), back.spawn_chance()), (24000, 3));
}

#[test]
fn random_sequences_match_vanilla_streams_and_codec() {
    let g = golden();
    let mut rs = RandomSequences::default();
    {
        use mc_core::random::RandomSource;
        rs.get(&id("minecraft:foo"), 12345).next_int();
    }
    rs.get(&id("bar:baz/q"), 12345);
    assert_eq!(RandomSequences::type_().encode.as_ref()(&rs).unwrap(), snbt(&g, "random_sequences_1"));
    rs.set_seed_defaults(5, false, false);
    rs.reset(&id("a:b"), 77);
    rs.reset_with(&id("c:d"), 77, -9, true, false);
    assert_eq!(RandomSequences::type_().encode.as_ref()(&rs).unwrap(), snbt(&g, "random_sequences_2"));
    // decode -> encode is the identity, and the restored stream continues where it left off
    let ty = RandomSequences::type_();
    let decoded = (ty.decode)(&snbt(&g, "random_sequences_2")).map_err(|e| e.error).unwrap();
    assert_eq!((ty.encode)(&decoded).unwrap(), snbt(&g, "random_sequences_2"));
    assert_eq!((decoded.salt(), decoded.include_world_seed(), decoded.include_sequence_id()), (5, false, false));
}

#[test]
fn random_sequences_drawing_marks_dirty() {
    use mc_core::random::RandomSource;
    use mc_game::world::level::saveddata::SavedData;
    let mut rs = RandomSequences::default();
    rs.get(&id("a:b"), 1);
    assert!(!rs.is_dirty(), "creating a sequence alone does not dirty");
    rs.get(&id("a:b"), 1).next_double();
    assert!(rs.is_dirty());
}

#[test]
fn random_sequences_partial_decode() {
    let g = golden();
    let ty = RandomSequences::type_();
    let bad = mc_nbt::snbt::parse("{salt:1,sequences:{\"a:b\":{source:[L;1L,2L]},\"c:d\":{source:[L;1L]},\"e:f\":5}}").unwrap();
    let partial = (ty.decode)(&bad).unwrap_err();
    assert!(g["random_sequences_partial"].ends_with("complete=false"));
    let kept = partial.value.expect("good entries are kept");
    assert_eq!((ty.encode)(&kept).unwrap(), mc_nbt::snbt::parse(g["random_sequences_partial"].split(" | ").next().unwrap()).unwrap());
}

#[test]
fn clock_state_codec() {
    let g = golden();
    let enc = |c: ClockState| c.encode();
    assert_eq!(enc(ClockState { total_ticks: 7, partial_tick: 0.0, rate: 1.0, paused: false }), snbt(&g, "clock_default"));
    assert_eq!(enc(ClockState { total_ticks: 7, partial_tick: 0.25, rate: 2.5, paused: true }), snbt(&g, "clock_all"));
    assert_eq!(enc(ClockState { total_ticks: 7, partial_tick: -0.0, rate: 1.0, paused: false }), snbt(&g, "clock_neg_zero_partial"));
    assert!(ClockState::decode(&mc_nbt::snbt::parse("{total_ticks:1L,rate:0.0f}").unwrap()).is_err());
    assert!(ClockState::decode(&mc_nbt::snbt::parse("{total_ticks:1L,rate:-1.0f}").unwrap()).is_err());
    let lenient = ClockState::decode(&mc_nbt::snbt::parse("{total_ticks:3,paused:2b,partial_tick:1}").unwrap()).unwrap();
    assert_eq!(lenient, ClockState { total_ticks: 3, partial_tick: 1.0, rate: 1.0, paused: true });
}

struct Ctx;
impl FunctionHost for Ctx {
    fn execute_function(&mut self, _: &Identifier) {}
    fn execute_function_tag(&mut self, _: &Identifier) {}
}

#[test]
fn timer_queue_codec_matches_vanilla() {
    let g = golden();
    let mut q = TimerQueue::<Ctx>::new();
    q.schedule("x", 7, Box::new(FunctionCallback { function_id: id("a:b") }));
    q.schedule("y", 3, Box::new(FunctionTagCallback { tag_id: id("t:u") }));
    q.schedule("x", 3, Box::new(FunctionCallback { function_id: id("c:d") }));
    let ty = TimerQueue::<Ctx>::type_();
    let tag = (ty.encode)(&q).unwrap();
    assert_eq!(tag, snbt(&g, "timer_queue"));
    let back = (ty.decode)(&tag).map_err(|e| e.error).unwrap();
    assert_eq!((ty.encode)(&back).unwrap(), tag);
    let _ = TimerCallbacks::<Ctx>::server_callbacks();
}
