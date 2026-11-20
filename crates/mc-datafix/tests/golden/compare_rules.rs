//! The four comparison rules of `common/compare.rs` on hand-made outcomes: result tree, `NbtIo` bytes (key order),
//! exception and ERROR log events.

use crate::compare::{Mismatch, Outcome, Status, compare, errors_hash, report, sha256_hex};
use mc_nbt::{CompoundTag, compound};

fn ok(tag: CompoundTag) -> Outcome {
    Outcome {
        status: Status::Ok {
            bytes: mc_nbt::nbt_io::to_bytes(&tag),
            tag,
        },
        error_count: 0,
        errors_hash: None,
    }
}

fn threw(class: &str, message: &str) -> Outcome {
    Outcome {
        status: Status::Threw {
            class: class.into(),
            message_hash: sha256_hex(message.as_bytes()),
        },
        error_count: 0,
        errors_hash: None,
    }
}

/// A tag holding `keys` whose `HashMap` table grew to 32 slots (13 keys were present once): the same entries iterate in
/// another order than in a table of 16.
fn grown(keys: &[String]) -> CompoundTag {
    let mut t = CompoundTag::new();
    for i in 0..13 {
        t.put(format!("churn{i}"), 0i32);
    }
    for i in 0..13 {
        t.remove(&format!("churn{i}"));
    }
    for k in keys {
        t.put(k.clone(), 1i32);
    }
    t
}

fn plain(keys: &[String]) -> CompoundTag {
    keys.iter().map(|k| (k.clone(), 1i32)).collect()
}

/// Two keys that a table of 16 and a table of 32 slots iterate in opposite order.
fn reordered_pair() -> Vec<String> {
    let names: Vec<String> = (0..60).map(|i| format!("key{i}")).collect();
    for a in &names {
        for b in &names {
            let pair = vec![a.clone(), b.clone()];
            let order = |t: &CompoundTag| -> Vec<String> {
                t.iter_java_order()
                    .into_iter()
                    .map(|(k, _)| k.as_lossy_str().to_owned())
                    .collect()
            };
            if a != b && order(&plain(&pair)) != order(&grown(&pair)) {
                return pair;
            }
        }
    }
    panic!("no key pair changes order with the table size");
}

#[test]
fn equal_outcomes_compare_equal() {
    let tag = compound! { "id" => "minecraft:stone", "Count" => 1i8 };
    assert_eq!(compare(&ok(tag.clone()), &ok(tag)), Ok(()));
    assert_eq!(compare(&threw("a.B", "m"), &threw("a.B", "m")), Ok(()));
}

#[test]
fn a_different_tree_is_tree_differs() {
    let a = compound! { "id" => "minecraft:stone" };
    let b = compound! { "id" => "minecraft:dirt" };
    assert_eq!(compare(&ok(a), &ok(b)), Err(Mismatch::TreeDiffers));
}

#[test]
fn the_same_tree_in_another_key_order_is_order_only() {
    let keys = reordered_pair();
    let result = compare(&ok(plain(&keys)), &ok(grown(&keys)));
    assert_eq!(result, Err(Mismatch::OrderOnly));
    assert!(result.unwrap_err().is_order_only());
}

#[test]
fn the_key_order_of_block_state_properties_is_not_compared() {
    let keys = reordered_pair();
    let state = |properties: CompoundTag| {
        let mut state = CompoundTag::new();
        state.put("Name", "minecraft:lever");
        state.put("Properties", properties);
        state
    };
    let (a, b) = (state(plain(&keys)), state(grown(&keys)));
    assert_eq!(compare(&ok(a), &ok(b)), Ok(()));
    let other = |inner: CompoundTag| compound! { "Name" => "minecraft:lever", "Other" => inner };
    assert_eq!(
        compare(&ok(other(plain(&keys))), &ok(other(grown(&keys)))),
        Err(Mismatch::OrderOnly)
    );
}

#[test]
fn exceptions_compare_by_class_and_message() {
    let tag = compound! { "a" => 1i8 };
    assert!(matches!(
        compare(&ok(tag), &threw("java.lang.IllegalStateException", "x")),
        Err(Mismatch::Status { .. })
    ));
    assert_eq!(
        compare(&threw("a.B", "m"), &threw("a.C", "m")),
        Err(Mismatch::ExceptionClass {
            vanilla: "a.B".into(),
            ours: "a.C".into()
        })
    );
    assert!(matches!(
        compare(&threw("a.B", "m"), &threw("a.B", "other")),
        Err(Mismatch::ExceptionMessage { .. })
    ));
}

#[test]
fn error_log_events_compare_by_count_and_message_hash() {
    let tag = compound! { "a" => 1i8 };
    let logged = |messages: &[&str]| {
        let messages: Vec<String> = messages.iter().map(ToString::to_string).collect();
        let mut o = ok(tag.clone());
        o.error_count = messages.len();
        o.errors_hash = errors_hash(&messages);
        o
    };
    assert_eq!(compare(&logged(&["x"]), &logged(&["x"])), Ok(()));
    assert_eq!(
        compare(&logged(&["x"]), &logged(&[])),
        Err(Mismatch::ErrorCount {
            vanilla: 1,
            ours: 0
        })
    );
    assert!(matches!(
        compare(&logged(&["x"]), &logged(&["y"])),
        Err(Mismatch::ErrorMessages { .. })
    ));
}

#[test]
fn the_report_counts_tree_differs_and_order_only() {
    let failures = vec![
        ("a".to_string(), Mismatch::TreeDiffers),
        ("b".to_string(), Mismatch::OrderOnly),
        ("c".to_string(), Mismatch::OrderOnly),
    ];
    let text = report(&failures, 10, 2);
    assert!(
        text.starts_with("3 of 10 differ (1 tree-differs, 2 order-only)"),
        "{text}"
    );
    assert!(text.contains("a: tree-differs") && text.contains("b: order-only"));
    assert!(!text.contains("c: "), "only the first two are listed");
}
