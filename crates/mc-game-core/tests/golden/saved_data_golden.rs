//! Codecs of the saved-data types against the real classes (`fixtures/SavedDataGolden.java` -> `saved_data.golden`).

use std::collections::HashMap;

use mc_core::ident::Identifier;
use mc_game::server::bossevents::CustomBossEvents;
use mc_game::server::players::player_files;
use mc_game::stats::ServerStatsCounter;
use mc_game::stats::stat::by_names;
use mc_game::util::timer_queue::{
    FunctionCallback, FunctionHost, FunctionTagCallback, TimerCallbacks, TimerQueue,
};
use mc_game::world::clock::ClockState;
use mc_game::world::entity::raid::raids::raids_type;
use mc_game::world::level::dimension::end::ender_dragon_fight_type;
use mc_game::world::level::saveddata::command_storage::Container;
use mc_game::world::level::saveddata::maps::MapIndex;
use mc_game::world::level::saveddata::random_sequences::RandomSequences;
use mc_game::world::level::saveddata::stopwatches::Stopwatches;
use mc_game::world::level::saveddata::wandering_trader_data::WanderingTraderData;
use mc_game::world::level::saveddata::weather_data::WeatherData;
use mc_game::world::level::saveddata::world_border::WorldBorder;
use mc_game::world::level::saveddata::{SavedData, SavedDataType};
use mc_game::world::level::ticket_storage;
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_world::level_storage::SavedDataStorage;

fn golden() -> HashMap<String, String> {
    include_str!("../../../mc-game/tests/fixtures/saved_data.golden")
        .lines()
        .map(|l| l.split_once('\t').unwrap())
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
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
    assert_eq!(
        WeatherData::new(1, 2, 3, true, false).encode().unwrap(),
        snbt(&g, "weather")
    );
    assert_eq!(
        WeatherData::decode(&snbt(&g, "weather"))
            .map_err(|e| e.error)
            .unwrap()
            .get_rain_time(),
        2
    );
    assert_eq!(
        WanderingTraderData::default().encode().unwrap(),
        snbt(&g, "wandering_default")
    );
    assert_eq!(
        WanderingTraderData::new(1, 25).encode().unwrap(),
        snbt(&g, "wandering_delay")
    );
    assert_eq!(
        WanderingTraderData::new(24000, 3).encode().unwrap(),
        snbt(&g, "wandering_chance")
    );
    let back = WanderingTraderData::decode(&snbt(&g, "wandering_chance"))
        .map_err(|e| e.error)
        .unwrap();
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
    assert_eq!(
        RandomSequences::type_().encode.as_ref()(&rs).unwrap(),
        snbt(&g, "random_sequences_1")
    );
    rs.set_seed_defaults(5, false, false);
    rs.reset(&id("a:b"), 77);
    rs.reset_with(&id("c:d"), 77, -9, true, false);
    assert_eq!(
        RandomSequences::type_().encode.as_ref()(&rs).unwrap(),
        snbt(&g, "random_sequences_2")
    );
    // decode -> encode is the identity, and the restored stream continues where it left off
    let ty = RandomSequences::type_();
    let decoded = (ty.decode)(&snbt(&g, "random_sequences_2"))
        .map_err(|e| e.error)
        .unwrap();
    assert_eq!(
        (ty.encode)(&decoded).unwrap(),
        snbt(&g, "random_sequences_2")
    );
    assert_eq!(
        (
            decoded.salt(),
            decoded.include_world_seed(),
            decoded.include_sequence_id()
        ),
        (5, false, false)
    );
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
    let bad = mc_nbt::snbt::parse(
        "{salt:1,sequences:{\"a:b\":{source:[L;1L,2L]},\"c:d\":{source:[L;1L]},\"e:f\":5}}",
    )
    .unwrap();
    let partial = (ty.decode)(&bad).unwrap_err();
    assert!(g["random_sequences_partial"].ends_with("complete=false"));
    let kept = partial.value.expect("good entries are kept");
    assert_eq!(
        (ty.encode)(&kept).unwrap(),
        mc_nbt::snbt::parse(g["random_sequences_partial"].split(" | ").next().unwrap()).unwrap()
    );
}

#[test]
fn clock_state_codec() {
    let g = golden();
    let enc = |c: ClockState| c.encode();
    assert_eq!(
        enc(ClockState {
            total_ticks: 7,
            partial_tick: 0.0,
            rate: 1.0,
            paused: false
        }),
        snbt(&g, "clock_default")
    );
    assert_eq!(
        enc(ClockState {
            total_ticks: 7,
            partial_tick: 0.25,
            rate: 2.5,
            paused: true
        }),
        snbt(&g, "clock_all")
    );
    assert_eq!(
        enc(ClockState {
            total_ticks: 7,
            partial_tick: -0.0,
            rate: 1.0,
            paused: false
        }),
        snbt(&g, "clock_neg_zero_partial")
    );
    assert!(
        ClockState::decode(&mc_nbt::snbt::parse("{total_ticks:1L,rate:0.0f}").unwrap()).is_err()
    );
    assert!(
        ClockState::decode(&mc_nbt::snbt::parse("{total_ticks:1L,rate:-1.0f}").unwrap()).is_err()
    );
    let lenient = ClockState::decode(
        &mc_nbt::snbt::parse("{total_ticks:3,paused:2b,partial_tick:1}").unwrap(),
    )
    .unwrap();
    assert_eq!(
        lenient,
        ClockState {
            total_ticks: 3,
            partial_tick: 1.0,
            rate: 1.0,
            paused: true
        }
    );
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
    q.schedule(
        "x",
        7,
        Box::new(FunctionCallback {
            function_id: id("a:b"),
        }),
    );
    q.schedule("y", 3, Box::new(FunctionTagCallback { tag_id: id("t:u") }));
    q.schedule(
        "x",
        3,
        Box::new(FunctionCallback {
            function_id: id("c:d"),
        }),
    );
    let ty = TimerQueue::<Ctx>::type_();
    let tag = (ty.encode)(&q).unwrap();
    assert_eq!(tag, snbt(&g, "timer_queue"));
    let back = (ty.decode)(&tag).map_err(|e| e.error).unwrap();
    assert_eq!((ty.encode)(&back).unwrap(), tag);
    let _ = TimerCallbacks::<Ctx>::server_callbacks();
}

/// Player lists of boss events are `Set.copyOf` sets (random order in vanilla): the golden has them sorted.
fn canon(tag: &Tag) -> Tag {
    match tag {
        Tag::Compound(c) => {
            let mut out = CompoundTag::new();
            for (k, v) in c.iter() {
                let mut v = canon(v);
                if k == "Players"
                    && let Tag::List(l) = &mut v
                {
                    let mut items: Vec<Tag> = l.iter().cloned().collect();
                    items.sort_by_key(|t| format!("{t:?}"));
                    *l = ListTag::from(items);
                }
                out.put(k.clone(), v);
            }
            Tag::Compound(out)
        }
        Tag::List(l) => Tag::List(l.iter().map(canon).collect()),
        other => other.clone(),
    }
}

/// Stopwatch values grow by the milliseconds elapsed since the decode: values within 50 of the input are the input.
fn unfuzz(out: Tag, input: &Tag) -> Tag {
    let Tag::Compound(mut out) = out else {
        return out;
    };
    let given = input
        .as_compound()
        .map(|c| c.get_compound_or_empty("stopwatches"));
    if let Some(Tag::Compound(inner)) = out.get_mut("stopwatches") {
        for (k, v) in inner.iter_mut() {
            let was = given.and_then(|g| g.get(k)).and_then(Tag::as_number);
            if let (Some(got), Some(was)) = (v.as_long(), was)
                && (got - was.long_value()).abs() <= 50
            {
                *v = Tag::Long(was.long_value());
            }
        }
    }
    Tag::Compound(out)
}

/// `ok <snbt>`, `partial <snbt>` or `err` for decoding `input` with `ty` and encoding the result.
fn codec_line<T: SavedData>(ty: &SavedDataType<T>, input: &str, fuzzy: bool) -> String {
    let tag = Tag::Compound(mc_nbt::snbt::parse_compound(input).unwrap());
    let (kind, value) = match (ty.decode)(&tag) {
        Ok(v) => ("ok", v),
        Err(p) => match p.value {
            Some(v) => ("partial", v),
            None => return "err".to_string(),
        },
    };
    let mut enc = canon(&(ty.encode)(&value).unwrap());
    if fuzzy {
        enc = unfuzz(enc, &tag);
    }
    format!("{kind} {enc}")
}

/// Every `<prefix> <input>` golden line against `codec_line`; the results are compared as parsed tags (compound order is
/// not significant to `CompoundTag`, the order of lists is).
fn check_codec_lines<T: SavedData>(prefix: &str, ty: &SavedDataType<T>, fuzzy: bool) -> usize {
    let mut n = 0;
    for (key, expected) in golden() {
        let Some(input) = key.strip_prefix(prefix).and_then(|r| r.strip_prefix(' ')) else {
            continue;
        };
        let got = codec_line(ty, input, fuzzy);
        let split = |l: &str| match l.split_once(' ') {
            Some((kind, tag)) => (kind.to_string(), Some(mc_nbt::snbt::parse(tag).unwrap())),
            None => (l.to_string(), None),
        };
        assert_eq!(
            split(&got),
            split(&expected),
            "{key}\n  ours:    {got}\n  vanilla: {expected}"
        );
        n += 1;
    }
    n
}

#[test]
fn command_storage_container_codec_matches_vanilla() {
    let ty = Container::type_("minecraft").unwrap();
    assert_eq!(check_codec_lines("command_storage", &ty, false), 8);
}

#[test]
fn stopwatches_codec_matches_vanilla() {
    assert_eq!(
        check_codec_lines("stopwatches", &Stopwatches::type_(), true),
        6
    );
}

#[test]
fn map_index_codec_matches_vanilla() {
    assert_eq!(check_codec_lines("map_index", &MapIndex::type_(), false), 7);
}

#[test]
fn raids_codec_matches_vanilla() {
    assert_eq!(check_codec_lines("raids", raids_type(), false), 11);
}

#[test]
fn custom_boss_events_codec_matches_vanilla() {
    assert_eq!(
        check_codec_lines("custom_boss_events", &CustomBossEvents::type_(), false),
        11
    );
}

/// The vanilla-written files of the committed fixture world decode and re-encode like the real classes do (vanilla's own
/// re-encode of `chunk_tickets.dat` does not keep the file's ticket order: it follows the hash table of the decoded map).
#[test]
fn fixture_world_files_re_encode_like_vanilla() {
    fn check<T: SavedData>(dir: &str, ty: &SavedDataType<T>, prepare: impl FnOnce(&mut T)) {
        let world = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../mc-world/tests/fixtures/level_storage/world/dimensions/minecraft")
            .join(dir)
            .join("data");
        let raw = SavedDataStorage::new(world).get(&ty.id).unwrap();
        let mut v = (ty.decode)(&raw).unwrap_or_else(|e| panic!("{}: {}", ty.id, e.error));
        prepare(&mut v);
        let name = ty.id.path();
        let key = format!("fixture dimensions/minecraft/{dir} {name}");
        assert_eq!((ty.encode)(&v).unwrap(), snbt(&golden(), &key), "{key}");
    }
    for dim in ["overworld", "the_nether", "the_end"] {
        check(dim, &WorldBorder::type_(), |b| b.apply_initial_settings(0));
        check(dim, raids_type(), |_| {});
        check(dim, &ticket_storage::saved_data_type(), |_| {});
    }
    check("the_end", ender_dragon_fight_type(), |_| {});
}

/// The files of the per-player JSON writer: Gson `setPrettyPrinting` with `htmlSafe` (`gson` rows, the `¶` stands for a newline).
#[test]
fn gson_writer_matches_vanilla() {
    let mut rows = 0;
    for (key, expected) in golden() {
        let Some(input) = key.strip_prefix("gson ") else {
            continue;
        };
        let json: serde_json::Value = serde_json::from_str(input).unwrap();
        assert_eq!(
            player_files::to_gson_pretty(&json).replace('\n', "¶"),
            expected,
            "{input}"
        );
        rows += 1;
    }
    assert_eq!(rows, 3);
}

/// `ServerStatsCounter.save()`: the stat types of vanilla's file come in a random order, ours are compared sorted.
#[test]
fn stats_file_matches_vanilla() {
    let stat = |ty: &str, name: &str| {
        by_names(&id(ty), &id(name)).unwrap_or_else(|e| panic!("{ty} {name}: {e}"))
    };
    let jump = stat("minecraft:custom", "minecraft:jump");
    let cases: [(&str, Vec<(mc_game::stats::stat::Stat, i32)>); 3] = [
        (
            "custom",
            vec![
                (jump, 7),
                (stat("minecraft:custom", "minecraft:walk_one_cm"), 1234),
                (stat("minecraft:custom", "minecraft:play_time"), i32::MAX),
            ],
        ),
        (
            "mixed",
            vec![
                (jump, 1),
                (stat("minecraft:mined", "minecraft:stone"), 5),
                (stat("minecraft:crafted", "minecraft:stick"), 3),
            ],
        ),
        ("empty", vec![]),
    ];
    let golden = golden();
    for (name, stats) in cases {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("stats.json");
        let mut counter = ServerStatsCounter::new(file.clone()).unwrap();
        for (s, n) in stats {
            counter.increment(s, n);
        }
        counter.save();
        let mut saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        let sorted: std::collections::BTreeMap<_, _> = saved["stats"]
            .as_object()
            .unwrap()
            .clone()
            .into_iter()
            .collect();
        saved["stats"] = serde_json::Value::Object(sorted.into_iter().collect());
        assert_eq!(
            player_files::to_gson_pretty(&saved).replace('\n', "¶"),
            golden[&format!("stats_file {name}")],
            "{name}"
        );
    }
}
