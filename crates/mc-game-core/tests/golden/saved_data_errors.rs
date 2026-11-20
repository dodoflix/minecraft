//! What loading a damaged saved-data file reports, against the real classes: `fixtures/SavedDataErrorsGolden.java` damages the
//! `data` tag of every saved-data type (a key removed or replaced by a value of another type, an unknown key, every key at
//! once, `data` of another tag type) and records the outcome of the codec parse (`ok`, `partial` or `err` with the text of
//! `Failed to parse saved data for '<type>': <text>`) and the encoded value.

use mc_core::ident::Identifier;
use mc_game::server::bossevents::CustomBossEvents;
use mc_game::util::timer_queue::{FunctionHost, TimerQueue};
use mc_game::world::clock::ServerClockManager;
use mc_game::world::entity::raid::raids::raids_type;
use mc_game::world::level::dimension::end::ender_dragon_fight_type;
use mc_game::world::level::gamerules::GameRuleMap;
use mc_game::world::level::saveddata::command_storage::Container;
use mc_game::world::level::saveddata::maps::{MapIndex, MapItemSavedData};
use mc_game::world::level::saveddata::random_sequences::RandomSequences;
use mc_game::world::level::saveddata::stopwatches::Stopwatches;
use mc_game::world::level::saveddata::wandering_trader_data::WanderingTraderData;
use mc_game::world::level::saveddata::weather_data::WeatherData;
use mc_game::world::level::saveddata::world_border::WorldBorder;
use mc_game::world::level::saveddata::{SavedData, SavedDataType};
use mc_game::world::level::ticket_storage;
use mc_game::world::scores::ScoreboardSaveData;
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_protocol::components::MapId;

struct Ctx;
impl FunctionHost for Ctx {
    fn execute_function(&mut self, _: &Identifier) {}
    fn execute_function_tag(&mut self, _: &Identifier) {}
}

/// Player lists and the colors of a map are compared sorted and shortened like the recording.
fn canon(tag: &Tag) -> Tag {
    match tag {
        Tag::Compound(c) => {
            let mut out = CompoundTag::new();
            for (k, v) in c.iter() {
                let mut v = canon(v);
                if (k == "Players" || k == "heroes_of_the_village")
                    && let Tag::List(l) = &mut v
                {
                    let mut items: Vec<Tag> = l.iter().cloned().collect();
                    items.sort_by_key(|t| format!("{t:?}"));
                    *l = ListTag::from(items);
                }
                if k == "colors"
                    && let Tag::ByteArray(b) = &v
                {
                    v = Tag::ByteArray(b.iter().copied().take(8).collect());
                }
                out.put(k.clone(), v);
            }
            Tag::Compound(out)
        }
        Tag::List(l) => Tag::List(l.iter().map(canon).collect()),
        other => other.clone(),
    }
}

/// `ok\t<encoded>`, `partial\t<message>\t<encoded>` or `err\t<message>`: `SavedDataStorage.readSavedData` on `data`.
fn outcome<T: SavedData>(ty: &SavedDataType<T>, data: &Tag, encode: bool) -> String {
    let enc = |value: &T| {
        if encode {
            canon(&(ty.encode)(value).unwrap_or_else(|e| panic!("encode: {e}"))).to_string()
        } else {
            "-".to_string()
        }
    };
    match (ty.decode)(data) {
        Ok(value) => format!("ok\t{}", enc(&value)),
        Err(partial) => match &partial.value {
            Some(value) => format!("partial\t{}\t{}", partial.error, enc(value)),
            None => format!("err\t{}", partial.error),
        },
    }
}

/// The result of one recorded line in ours, per type.
fn ours(name: &str, data: &Tag) -> String {
    match name {
        "custom_boss_events" => outcome(&CustomBossEvents::type_(), data, true),
        "random_sequences" => outcome(&RandomSequences::type_(), data, true),
        "stopwatches" => outcome(&Stopwatches::type_(), data, false),
        "world_clocks" => outcome(
            &ServerClockManager::type_(&[
                Identifier::parse("minecraft:overworld").unwrap(),
                Identifier::parse("minecraft:the_end").unwrap(),
            ]),
            data,
            true,
        ),
        "raids" => outcome(raids_type(), data, true),
        "chunk_tickets" => outcome(&ticket_storage::saved_data_type(), data, true),
        "world_border" => outcome(&WorldBorder::type_(), data, true),
        "ender_dragon_fight" => outcome(ender_dragon_fight_type(), data, true),
        "game_rules" => outcome(&GameRuleMap::type_(), data, true),
        "wandering_trader" => outcome(&WanderingTraderData::type_(), data, true),
        "weather" => outcome(&WeatherData::type_(), data, true),
        "map_index" => outcome(&MapIndex::type_(), data, true),
        "map_data" => outcome(&MapItemSavedData::type_(MapId(1)), data, true),
        "command_storage" => outcome(&Container::type_("minecraft").unwrap(), data, true),
        "scheduled_events" => outcome(&TimerQueue::<Ctx>::type_(), data, true),
        "scoreboard" => outcome(&ScoreboardSaveData::type_(), data, true),
        other => panic!("no saved data type {other}"),
    }
}

/// Message and encoded value of a result line; the encoded values compare as tags (compound order is not significant).
fn split(result: &str) -> (String, String, Option<Tag>) {
    let mut fields = result.split('\t');
    let kind = fields.next().unwrap().to_string();
    let (message, encoded) = match kind.as_str() {
        "ok" => (String::new(), fields.next()),
        "partial" => (fields.next().unwrap().to_string(), fields.next()),
        _ => (fields.next().unwrap_or("").to_string(), None),
    };
    let tag = encoded
        .filter(|e| *e != "-")
        .map(|e| canon(&mc_nbt::snbt::parse(e).unwrap_or_else(|e| panic!("{e:?}"))));
    (kind, message, tag)
}

#[test]
fn damaged_saved_data_reports_like_vanilla() {
    let golden =
        mc_testkit::include_gz!("../../../mc-game/tests/fixtures/saved_data_errors.golden.gz");
    let mut total = 0;
    let mut differing: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for line in golden.lines() {
        let (name, rest) = line.split_once('\t').unwrap();
        let (input, vanilla) = rest.split_once('\t').unwrap();
        // its record lives in mc-world and its stems need the data pack registries: see `world_gen_settings_errors` in mc-game
        if name == "world_gen_settings" {
            continue;
        }
        let vanilla = vanilla.replace('¶', "\n");
        let data = mc_nbt::snbt::parse(input).unwrap_or_else(|e| panic!("{name} {input}: {e:?}"));
        let ours = ours(name, &data);
        total += 1;
        if split(&ours) != split(&vanilla) {
            differing
                .entry(name.to_string())
                .or_default()
                .push(format!("{input}\n  ours:    {ours}\n  vanilla: {vanilla}"));
        }
    }
    let report: Vec<String> = differing
        .iter()
        .flat_map(|(name, cases)| cases.iter().take(2).map(move |c| format!("{name}: {c}")))
        .collect();
    assert!(
        report.is_empty(),
        "of {total} damaged files, these differ from vanilla:\n{}",
        report.join("\n")
    );
}
