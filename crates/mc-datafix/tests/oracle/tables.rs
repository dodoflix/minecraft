//! The committed lookup tables (`src/fixes/data/*.tsv.gz`, `fixtures/table_accessors.tsv.gz`) against the live vanilla
//! classes.

use crate::oracle_client::{repo_root, run_verb};
use std::collections::BTreeMap;
use std::path::Path;

/// The dump of every table class, by file name.
fn dump(dir: &str) -> BTreeMap<String, String> {
    let dir = repo_root().join("target/tmp").join(dir);
    let _ = std::fs::remove_dir_all(&dir);
    run_verb(&["tables", dir.to_str().expect("UTF-8 path")]);
    let files = std::fs::read_dir(&dir)
        .expect("the dump directory")
        .map(|e| {
            let path = e.expect("entry").path();
            (
                path.file_name()
                    .and_then(|n| n.to_str())
                    .expect("name")
                    .to_string(),
                std::fs::read_to_string(&path).expect("UTF-8 dump"),
            )
        })
        .collect();
    std::fs::remove_dir_all(&dir).expect("remove the dump directory");
    files
}

#[test]
#[ignore = "oracle: runs vanilla's DataFixers in a JVM"]
fn table_dump_is_deterministic_and_matches_the_committed_data() {
    let first = dump("datafix-tables-1");
    let second = dump("datafix-tables-2");
    assert!(first == second, "two runs of tables differ");
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixes/data");
    let committed: BTreeMap<String, String> = std::fs::read_dir(&data)
        .expect("src/fixes/data")
        .map(|e| {
            let path = e.expect("entry").path();
            let name = path.file_name().and_then(|n| n.to_str()).expect("name");
            (
                name.strip_suffix(".gz").expect("a .gz file").to_string(),
                mc_testkit::read_gz(&path),
            )
        })
        .collect();
    assert_eq!(
        first.keys().collect::<Vec<_>>(),
        committed.keys().collect::<Vec<_>>(),
        "regenerate with scripts/datafix-oracle.sh --regen"
    );
    for (name, text) in &first {
        assert!(
            *text == committed[name],
            "{name} differs: regenerate with scripts/datafix-oracle.sh --regen"
        );
    }
}

#[test]
#[ignore = "oracle: runs vanilla's DataFixers in a JVM"]
fn table_accessors_match_the_recording() {
    let live = run_verb(&["table-accessors"]);
    let recorded = mc_testkit::include_gz!("../fixtures/table_accessors.tsv.gz");
    assert!(
        live == recorded,
        "regenerate with scripts/datafix-oracle.sh --regen"
    );
}
