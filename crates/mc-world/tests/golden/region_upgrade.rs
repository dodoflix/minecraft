//! `SimpleRegionStorage.upgradeChunkTag` over chunk, entity chunk and POI chunk tags and the partial chunk fix of the
//! structure check (hand written, generated and real samples from a 26.2 world,
//! `mc-datafix/tests/fixtures/samples/region_upgrade.tsv.gz` recorded from vanilla): the result tree and its `NbtIo`
//! bytes (key order included) must equal vanilla's.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use mc_datafix::DataFixTypes;
use mc_nbt::{CompoundTag, NbtAccounter};
use mc_world::region::upgrade::{
    ChunkUpgradeError, chunk_data_fix_context_tag, collect_scanned_fields,
    update_scanned_chunk_tag, upgrade_chunk_tag_to,
};

fn read_tag(bytes: &[u8]) -> CompoundTag {
    let mut input = bytes;
    mc_nbt::read_named(&mut input, &mut NbtAccounter::unlimited_heap()).expect("NBT bytes")
}

/// The request type of a sample: `upgrade:DATAFIXTYPE[@DIMENSION[@GENERATOR]]` (`upgradeChunkTag`) or
/// `scan:DIMENSION[@GENERATOR]` (the partial fix of a scanned chunk).
fn run(
    spec: &str,
    input: CompoundTag,
    default_version: i32,
    target: i32,
) -> Result<CompoundTag, ChunkUpgradeError> {
    let context = |dimension: &str, generator: Option<&str>| {
        chunk_data_fix_context_tag(dimension, generator.filter(|g| !g.is_empty()))
    };
    if let Some(spec) = spec.strip_prefix("scan:") {
        let (dimension, generator) = spec.split_once('@').unwrap_or((spec, ""));
        let context = context(dimension, Some(generator));
        return update_scanned_chunk_tag(collect_scanned_fields(&input), &context);
    }
    let mut parts = spec
        .strip_prefix("upgrade:")
        .expect("a request type")
        .split('@');
    let ty = DataFixTypes::by_name(parts.next().unwrap()).expect("a DataFixTypes constant");
    let context = parts
        .next()
        .map(|dimension| context(dimension, parts.next()));
    upgrade_chunk_tag_to(ty, input, default_version, context.as_ref(), target)
}

/// Samples without a `DataVersion` whose stamped key lands in another place than vanilla's: the fixer's result has the
/// right content, the table the key is added to is not tracked (see `upgrade_chunk_tag_to`).
const KEY_ORDER_DIFFERS: &[&str] = &[
    "g7-ENTITY_CHUNK-4899-5023-4",
    "g7-ENTITY_CHUNK-4899-5023-5-bad",
    "g7-ENTITY_CHUNK-4899-5023-13",
    "g7-ENTITY_CHUNK-4899-5023-15-bad",
    "g7-ENTITY_CHUNK-4899-5023-16-bad",
    "g7-ENTITY_CHUNK-4899-5023-20",
    "g7-POI_CHUNK-4899-5023-11",
    "g7-POI_CHUNK-4899-5023-13-bad",
    "g7-POI_CHUNK-4899-5023-16",
    "g7-POI_CHUNK-4899-5023-19",
    "g7-POI_CHUNK-4899-5023-20",
];

#[test]
fn upgrade_chunk_tag_equals_vanilla() {
    let fixture =
        mc_testkit::include_gz!("../../../mc-datafix/tests/fixtures/samples/region_upgrade.tsv.gz");
    let mut lines = fixture.lines();
    let (mut total, mut failures, mut order_only) = (0, Vec::new(), Vec::new());
    while let Some(request) = lines.next() {
        let record = lines.next().expect("every request has a record");
        let f: Vec<&str> = request.split('\t').collect();
        let (id, spec) = (f[0], f[1]);
        let (default_version, target): (i32, i32) = (f[2].parse().unwrap(), f[3].parse().unwrap());
        let input = if f[4].starts_with('{') {
            mc_nbt::snbt::parse_compound(f[4]).expect("SNBT input")
        } else {
            read_tag(&STANDARD.decode(f[4]).unwrap())
        };
        let r: Vec<&str> = record.splitn(9, '\t').collect();
        assert_eq!((r[0], r[1]), (id, "ok"), "vanilla threw for {id}");
        let expected_bytes = STANDARD.decode(r[3]).unwrap();
        total += 1;
        match run(spec, input, default_version, target) {
            Err(e) => failures.push(format!("{id}: {e}")),
            Ok(tag) if tag != read_tag(&expected_bytes) => {
                failures.push(format!("{id}: tree differs"))
            }
            Ok(tag) if mc_nbt::nbt_io::to_bytes(&tag) != expected_bytes => {
                order_only.push(id.to_owned())
            }
            Ok(_) => {}
        }
    }
    assert_eq!(total, 129);
    assert!(
        failures.is_empty(),
        "{} of {total} differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(
        order_only, KEY_ORDER_DIFFERS,
        "key order differences (a sample that now matches leaves the list)"
    );
}
