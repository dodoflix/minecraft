//! What reading a damaged `world_gen_settings.dat` reports, against the real classes: the `world_gen_settings` lines of
//! `fixtures/saved_data_errors.golden.gz` (generator `fixtures/SavedDataErrorsGolden.java`), with the dimension stems
//! decoded by `LevelStem.CODEC` over the data pack registries. A decode that fails keeps no partial value
//! (`readExistingSavedData`), and a `dimensions` map without an overworld stem makes `WorldDimensions` throw.

use crate::common;

use mc_game::server::world_dimensions::DimensionDecoder;
use mc_game::world::level::saveddata::encode_codec;
use mc_nbt::{CompoundTag, Tag};
use mc_world::level_storage::{ReadError, WorldDataConfiguration, WorldGenSettings};

/// `ok\t<encoded>`, `err\t<message>` or `throw\t<exception>`.
fn outcome(data: &Tag, stem: &mc_dfu::Codec<Tag>) -> String {
    let data = match data {
        Tag::Compound(_) => data.clone(),
        _ => Tag::Compound(CompoundTag::new()),
    };
    match WorldGenSettings::decode(&data, stem) {
        Ok(settings) => {
            let encoded = encode_codec(&WorldGenSettings::codec(stem), &settings).unwrap();
            format!("ok\t{encoded}")
        }
        Err(ReadError::Error(message)) => format!("err\t{message}"),
        Err(ReadError::Thrown(message)) => format!("throw\t{message}"),
    }
}

/// Kind, message and encoded value of a result line; the encoded values compare as tags (compound order is not
/// significant).
fn split(result: &str) -> (String, String, Option<Tag>) {
    let (kind, rest) = result.split_once('\t').unwrap();
    match kind {
        "ok" => (
            kind.to_owned(),
            String::new(),
            Some(mc_nbt::snbt::parse(rest).unwrap_or_else(|e| panic!("{e:?}"))),
        ),
        _ => (kind.to_owned(), rest.to_owned(), None),
    }
}

#[test]
fn damaged_world_gen_settings_report_like_vanilla() {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (_, world) = common::load_world(dir.path(), WorldDataConfiguration::DEFAULT, true).unwrap();
    let stem = DimensionDecoder::new(world.registries.composite_access())
        .unwrap()
        .stored_stem_codec();
    let golden = mc_testkit::include_gz!("../fixtures/saved_data_errors.golden.gz");
    let mut total = 0;
    let mut differing = Vec::new();
    for line in golden.lines() {
        let (name, rest) = line.split_once('\t').unwrap();
        if name != "world_gen_settings" {
            continue;
        }
        let (input, vanilla) = rest.split_once('\t').unwrap();
        let mut vanilla = vanilla.replace('¶', "\n");
        // `readExistingSavedData` keeps no partial value: a partial result falls back like an error
        if let Some(partial) = vanilla.strip_prefix("partial\t") {
            vanilla = format!("err\t{}", partial.split('\t').next().unwrap());
        }
        let data = mc_nbt::snbt::parse(input).unwrap_or_else(|e| panic!("{input}: {e:?}"));
        let ours = outcome(&data, &stem);
        total += 1;
        if split(&ours) != split(&vanilla) {
            differing.push(format!("{input}\n  ours:    {ours}\n  vanilla: {vanilla}"));
        }
    }
    assert_eq!(total, 115);
    assert!(
        differing.is_empty(),
        "of {total} damaged files, these differ from vanilla:\n{}",
        differing.join("\n")
    );
}
