//! The fixture of one `DataFixTypes` constant. Text lines (a fixture is the gzip of them):
//!
//! ```text
//! S  ID  TYPE  FROM  BASE64          a sample: the NbtIo bytes of the input, stored once
//! R  ID  TO  STATUS  SHA256  EXCEPTION  EXCEPTION_HASH  ERRORS  ERRORS_HASH   what vanilla's update to TO answered
//! ```
//!
//! `R` lines are the oracle's records (`DataFixerOracle.java`) without the result bytes and the SNBT: `SHA256` is that of
//! the `NbtIo` bytes of the result, which equal bytes mean equal trees and equal key order.

use base64::{Engine, engine::general_purpose::STANDARD};

/// Pairs the oracle's requests with its records (line for line) into the lines of a fixture.
pub fn pack(requests: &str, records: &str) -> Result<String, String> {
    let (reqs, recs): (Vec<&str>, Vec<&str>) =
        (requests.lines().collect(), records.lines().collect());
    if reqs.len() != recs.len() {
        return Err(format!(
            "{} requests but {} records",
            reqs.len(),
            recs.len()
        ));
    }
    let mut out = String::from(
        "# S ID TYPE FROM BASE64 | R ID TO STATUS SHA256 EXCEPTION EXCEPTION_HASH ERRORS ERRORS_HASH\n",
    );
    let mut last = "";
    for (req, rec) in reqs.iter().zip(&recs) {
        let r: Vec<&str> = req.split('\t').collect();
        let k: Vec<&str> = rec.splitn(9, '\t').collect();
        let [id, ty, from, to, input] = r[..] else {
            return Err(format!("malformed request: {req:.80}"));
        };
        let [
            rid,
            status,
            sha,
            _bytes,
            exc,
            exc_hash,
            errs,
            errs_hash,
            _snbt,
        ] = k[..]
        else {
            return Err(format!("malformed record: {rec:.80}"));
        };
        if id != rid {
            return Err(format!("record of {rid} answers the request of {id}"));
        }
        if id != last {
            out.push_str(&format!("S\t{id}\t{ty}\t{from}\t{input}\n"));
            last = id;
        }
        out.push_str(&format!(
            "R\t{id}\t{to}\t{status}\t{sha}\t{exc}\t{exc_hash}\t{errs}\t{errs_hash}\n"
        ));
    }
    Ok(out)
}

/// The `NbtIo` bytes of sample `id` of a fixture.
pub fn dump(fixture: &str, id: &str) -> Option<Vec<u8>> {
    fixture.lines().find_map(|l| {
        let f: Vec<&str> = l.split('\t').collect();
        match f[..] {
            ["S", sid, _, _, b64] if sid == id => STANDARD.decode(b64).ok(),
            _ => None,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_records_become_samples_and_answers() {
        let reqs = "a\tCHUNK\t-1\t1343\tQUJD\na\tCHUNK\t-1\t5023\tQUJD\nb\tLEVEL\t5\t5023\tREVG\n";
        let recs = "a\tok\tSHA1\tXX\t-\t-\t0\t-\tsnbt\na\tthrew\t-\t-\tjava.lang.X\tH\t1\tEH\t-\nb\tok\tSHA2\tYY\t-\t-\t0\t-\t{}\n";
        let text = pack(reqs, recs).unwrap();
        let lines: Vec<&str> = text.lines().skip(1).collect();
        assert_eq!(
            lines,
            [
                "S\ta\tCHUNK\t-1\tQUJD",
                "R\ta\t1343\tok\tSHA1\t-\t-\t0\t-",
                "R\ta\t5023\tthrew\t-\tjava.lang.X\tH\t1\tEH",
                "S\tb\tLEVEL\t5\tREVG",
                "R\tb\t5023\tok\tSHA2\t-\t-\t0\t-",
            ]
        );
        assert_eq!(dump(&text, "b"), Some(b"DEF".to_vec()));
        assert_eq!(dump(&text, "c"), None);
    }

    #[test]
    fn a_record_of_another_request_is_refused() {
        assert!(pack("a\tT\t1\t2\tQQ\n", "b\tok\t-\t-\t-\t-\t0\t-\t-\n").is_err());
        assert!(pack("a\tT\t1\t2\tQQ\n", "").is_err());
    }
}
