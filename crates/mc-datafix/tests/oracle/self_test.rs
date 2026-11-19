//! The oracle against itself: vanilla through the batch JVM of this process (stdin/stdout), through a second JVM in
//! file mode and through the one-shot `fix` verb must agree under every comparison rule. This proves the protocol, the
//! parsing and `compare` before our chain exists; with the chain, the same requests go through `compare::run_ours`.

use crate::compare::{Mismatch, Outcome, Request, Status, compare, report};
use crate::oracle_client::{oracle, parse_record, repo_root, run_verb};
use mc_nbt::{CompoundTag, ListTag, Tag, compound};
use std::path::PathBuf;

const ITEMS: &[&str] = &[
    "minecraft:stone",
    "minecraft:diamond_sword",
    "minecraft:potion",
    "minecraft:wool",
    "minecraft:spawn_egg",
    "minecraft:written_book",
    "minecraft:skull",
    "minecraft:banner",
    "minecraft:bed",
    "minecraft:log",
];
const FROM: &[i32] = &[1343, 2730, 3465];

fn item(i: usize) -> CompoundTag {
    let mut tag = compound! {
        "id" => ITEMS[i % ITEMS.len()],
        "Count" => 1 + (i % 64) as i8,
        "Damage" => (i % 16) as i16,
    };
    if i.is_multiple_of(3) {
        tag.put(
            "tag",
            compound! { "display" => compound! { "Name" => "{\"text\":\"sample\"}" } },
        );
    }
    tag
}

fn requests(n: usize) -> Vec<Request> {
    (0..n)
        .map(|i| {
            let from = FROM[i % FROM.len()];
            let (ty, input) = match i % 4 {
                0 => ("ref:item_stack", item(i)),
                1 => (
                    "ref:entity",
                    compound! { "id" => "minecraft:zombie", "Health" => 20i16, "Air" => (i % 300) as i16 },
                ),
                2 => (
                    "CHUNK",
                    compound! {
                        "Level" => compound! {
                            "xPos" => i as i32,
                            "zPos" => 7i32,
                            "Entities" => ListTag::new(),
                            "TileEntities" => ListTag::new(),
                        },
                    },
                ),
                _ => (
                    "PLAYER",
                    compound! {
                        "Health" => 20.0f32,
                        "Inventory" => ListTag::from(vec![Tag::Compound(item(i))]),
                    },
                ),
            };
            Request {
                id: format!("sample-{i}"),
                ty: ty.to_string(),
                from,
                to: 5023,
                input,
            }
        })
        .collect()
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("datafix-oracle");
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir.join(name)
}

fn unwrap_ok(o: &Outcome) -> (&CompoundTag, &[u8]) {
    match &o.status {
        Status::Ok { tag, bytes } => (tag, bytes),
        other => panic!("expected a result, got {other:?}"),
    }
}

#[test]
#[ignore = "oracle: runs vanilla's DataFixers in a JVM"]
fn batch_agrees_with_a_second_process_in_file_mode() {
    let requests = requests(1000);
    let first = oracle().batch(&requests);

    let input = scratch("batch-in.tsv");
    let output = scratch("batch-out.tsv");
    let mut text = String::new();
    for r in &requests {
        let bytes = mc_nbt::nbt_io::to_bytes(&r.input);
        text.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            r.id,
            r.ty,
            r.from,
            r.to,
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
        ));
    }
    std::fs::write(&input, text).expect("write the requests");
    run_verb(&[
        "batch",
        input.to_str().expect("utf-8 path"),
        output.to_str().expect("utf-8 path"),
    ]);
    let second: Vec<Outcome> = std::fs::read_to_string(&output)
        .expect("the oracle's output file")
        .lines()
        .map(|l| parse_record(l).1)
        .collect();
    assert_eq!(second.len(), requests.len());

    let failures: Vec<(String, Mismatch)> = requests
        .iter()
        .zip(first.iter().zip(&second))
        .filter_map(|(r, (a, b))| compare(a, b).err().map(|m| (r.id.clone(), m)))
        .collect();
    assert!(
        failures.is_empty(),
        "{}",
        report(&failures, requests.len(), 10)
    );

    let ok = first
        .iter()
        .filter(|o| matches!(o.status, Status::Ok { .. }))
        .count();
    assert!(ok > 900, "only {ok} of 1000 samples updated cleanly");
    // the fixers did something: a legacy item came out as a component item
    let (stack, _) = unwrap_ok(&first[0]);
    assert!(stack.contains("components"), "{stack:?}");
}

#[test]
#[ignore = "oracle: runs vanilla's DataFixers in a JVM"]
fn fix_verb_returns_the_bytes_of_the_batch_update() {
    let request = requests(1).remove(0);
    let batch = oracle().update(&request);
    let (_, batch_bytes) = unwrap_ok(&batch);

    let input = scratch("fix-in.nbt");
    let output = scratch("fix-out.nbt");
    std::fs::write(&input, mc_nbt::nbt_io::to_bytes(&request.input)).expect("write the input");
    let record = run_verb(&[
        "fix",
        &request.ty,
        &request.from.to_string(),
        &request.to.to_string(),
        input.to_str().expect("utf-8 path"),
        output.to_str().expect("utf-8 path"),
    ]);
    let (id, one_shot) = parse_record(record.trim_end());
    assert_eq!(id, "-");
    compare(&batch, &one_shot).expect("the verbs agree");
    assert_eq!(std::fs::read(&output).expect("result bytes"), batch_bytes);
    assert!(repo_root().join("scripts/datafix-oracle.sh").is_file());
}

#[test]
#[ignore = "oracle: runs vanilla's DataFixers in a JVM"]
fn an_unknown_type_is_a_thrown_exception_and_garbage_is_bad_input() {
    let mut r = requests(1).remove(0);
    r.ty = "NO_SUCH_TYPE".to_string();
    match oracle().update(&r).status {
        Status::Threw { class, .. } => assert_eq!(class, "java.lang.IllegalArgumentException"),
        other => panic!("{other:?}"),
    }
    let garbage = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, [9u8, 9, 9]);
    let line = format!("garbage\tCHUNK\t1343\t5023\t{garbage}\n");
    assert!(matches!(
        oracle().exchange("garbage", &line).status,
        Status::Bad(_)
    ));
}
