//! STEP samples of the JSON file types, `STATS` and `ADVANCEMENTS` (`fixtures/samples/json_files.tsv.gz`, recorded from
//! vanilla's `DataFixers` over `JsonOps` by `fixtures/samples/record.sh json_files 0`): the result text
//! (`JsonElement.toString()`, so object key order included) and the logged error count must equal vanilla's.

use crate::compare::{capture_errors, errors_hash};
use mc_datafix::{DataFixTypes, data_fixer};
use mc_dfu::{Dyn, Dynamic, JSON};

#[test]
fn json_file_fixes_equal_vanilla() {
    let fixture = mc_testkit::include_gz!("../fixtures/samples/json_files.tsv.gz");
    let mut lines = fixture.lines();
    let (mut total, mut failures) = (0, Vec::new());
    while let Some(request) = lines.next() {
        let record: Vec<&str> = lines
            .next()
            .expect("a record per request")
            .split('\t')
            .collect();
        let f: Vec<&str> = request.split('\t').collect();
        assert_eq!((f.len(), record.len()), (5, 9), "{request}");
        assert_eq!(f[0], record[0], "record of another request");
        let ty = DataFixTypes::by_name(f[1].strip_prefix("json:").expect("a json: type"))
            .expect("a DataFixTypes constant");
        let (from, to): (i32, i32) = (f[2].parse().unwrap(), f[3].parse().unwrap());
        let (ours, errors) = capture_errors(|| {
            let input = Dynamic::new(JSON, Dyn::from_json_str(f[4]).expect("strict JSON"));
            ty.update(data_fixer(), input, from, to)
                .value()
                .to_json_string()
        });
        total += 1;
        let vanilla = String::from_utf8(
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, record[3]).unwrap(),
        )
        .unwrap();
        let vanilla_errors = (record[7] != "-").then(|| record[7].to_string());
        if record[1] != "ok"
            || vanilla != ours
            || record[6] != errors.len().to_string()
            || vanilla_errors != errors_hash(&errors)
        {
            failures.push(format!(
                "{}: vanilla {vanilla} ({} errors), ours {} ({} errors)",
                f[0],
                record[6],
                ours.as_lossy_str(),
                errors.len()
            ));
        }
    }
    assert_eq!(total, 12);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
