//! Replay of recorded STEP samples: a fixture is a gzip of alternating lines, a batch request line
//! (`ID TYPE FROM TO BASE64`, or SNBT text for a hand written sample; the input of the oracle's `batch` verb) and the record vanilla's `DataFixers` answered
//! with. Every sample runs through our fixer and is compared by the four rules of `compare`.

use crate::compare::{
    Mismatch, Request, Status, compare, first_order_difference, parse_record, read_tag, report,
    run_ours,
};
use mc_datafix::data_fixer;
use mc_nbt::Tag;

/// Runs every sample of `fixture` (the decompressed text) and returns how many ran and the failures.
pub fn replay(fixture: &str) -> (usize, Vec<(String, Mismatch)>) {
    let mut lines = fixture.lines();
    let fixer = data_fixer();
    let (mut total, mut failures) = (0, Vec::new());
    while let Some(request) = lines.next() {
        let record = lines.next().expect("every request line has a record line");
        let f: Vec<&str> = request.split('\t').collect();
        assert_eq!(f.len(), 5, "malformed request line: {request}");
        let request = Request {
            id: f[0].to_string(),
            ty: f[1].to_string(),
            from: f[2].parse().expect("from"),
            to: f[3].parse().expect("to"),
            input: if f[4].starts_with('{') {
                mc_nbt::snbt::parse_compound(f[4]).expect("the input is SNBT")
            } else {
                let bytes =
                    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, f[4])
                        .expect("request bytes are base64");
                read_tag(&bytes).expect("the input is NBT")
            },
        };
        let (id, vanilla) = parse_record(record);
        assert_eq!(id, request.id, "record of another request");
        total += 1;
        let ours = run_ours(fixer, &request);
        if let Err(m) = compare(&vanilla, &ours) {
            let detail = match (&m, &vanilla.status, &ours.status) {
                (Mismatch::OrderOnly, Status::Ok { tag: v, .. }, Status::Ok { tag: o, .. }) => {
                    first_order_difference(&Tag::Compound(v.clone()), &Tag::Compound(o.clone()))
                        .map(|d| format!(" at {d}"))
                        .unwrap_or_default()
                }
                _ => String::new(),
            };
            failures.push((format!("{}{detail}", request.id), m));
        }
    }
    (total, failures)
}

/// `replay`, failing with the summary of `report` when a sample differs.
pub fn assert_replays(fixture: &str, expected_total: usize) {
    let (total, failures) = replay(fixture);
    assert_eq!(
        total, expected_total,
        "the fixture holds another number of samples"
    );
    assert!(failures.is_empty(), "{}", report(&failures, total, 4000));
}
