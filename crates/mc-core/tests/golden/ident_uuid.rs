//! Golden tests for `Identifier` and `uuid_util` against vanilla (fixtures/ident_uuid/{Ident,Uuid}Golden.java).
use std::cmp::Ordering;

use mc_core::ident::{self, Identifier};
use mc_core::uuid_util::*;
use uuid::Uuid;

const IDENT: &str = include_str!("../fixtures/ident_uuid/ident.txt");
const UUIDS: &str = include_str!("../fixtures/ident_uuid/uuid.txt");

/// Decodes the generators' escaping (`\uXXXX` per UTF-16 unit).
fn unesc(s: &str) -> String {
    let b: Vec<u16> = s.encode_utf16().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == u16::from(b'\\') {
            let hex = String::from_utf16(&b[i + 2..i + 6]).unwrap();
            out.push(u16::from_str_radix(&hex, 16).unwrap());
            i += 6;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf16(&out).unwrap()
}

/// Same escaping as the generator (units outside 0x20..0x7e and backslash).
fn esc(s: &str) -> String {
    s.encode_utf16()
        .map(|c| {
            if (0x20..=0x7e).contains(&c) && c != u16::from(b'\\') {
                char::from(c as u8).to_string()
            } else {
                format!("\\u{c:04x}")
            }
        })
        .collect()
}

fn fields(kind: &str, text: &'static str) -> impl Iterator<Item = Vec<&'static str>> {
    let prefix = format!("{kind}\t");
    text.lines()
        .filter(move |l| l.starts_with(&prefix))
        .map(|l| l.split('\t').skip(1).collect())
}

fn id_res(r: Result<Identifier, ident::IdentifierException>) -> String {
    match r {
        Ok(i) => format!("ok:{}", esc(&i.to_string())),
        Err(e) => format!("err:{}", esc(&e.to_string())),
    }
}

fn opt(i: Option<Identifier>) -> String {
    i.map_or("null".to_owned(), |i| esc(&i.to_string()))
}

#[test]
fn identifier_parse_matches_vanilla() {
    let mut n = 0;
    for f in fields("id", IDENT) {
        let input = unesc(f[0]);
        assert_eq!(
            opt(Identifier::try_parse(&input)),
            f[1],
            "try_parse {:?}",
            f[0]
        );
        assert_eq!(id_res(Identifier::parse(&input)), f[2], "parse {:?}", f[0]);
        assert_eq!(
            ident::is_valid_path(&input).to_string(),
            f[3],
            "is_valid_path {:?}",
            f[0]
        );
        assert_eq!(
            ident::is_valid_namespace(&input).to_string(),
            f[4],
            "is_valid_namespace {:?}",
            f[0]
        );
        if let Some(id) = Identifier::try_parse(&input) {
            assert_eq!(id.java_hash_code().to_string(), f[5], "hash {:?}", f[0]);
            assert_eq!(esc(&id.to_short_string()), f[6]);
            assert_eq!(esc(&id.to_language_key()), f[7]);
            assert_eq!(esc(&id.to_debug_file_name()), f[8]);
            assert_eq!(esc(&id.to_short_language_key()), f[9]);
            assert_eq!(
                esc(&id.to_language_key_prefixed_suffixed("pre", "suf")),
                f[10]
            );
        } else {
            assert_eq!(f.len(), 5);
        }
        n += 1;
    }
    assert!(n > 40);
}

#[test]
fn identifier_constructors_match_vanilla() {
    let mut n = 0;
    for f in fields("pair", IDENT) {
        let (ns, path) = (unesc(f[0]), unesc(f[1]));
        assert_eq!(
            id_res(Identifier::new(&ns, &path)),
            f[2],
            "new {:?} {:?}",
            f[0],
            f[1]
        );
        assert_eq!(
            opt(Identifier::try_build(&ns, &path)),
            f[3],
            "try_build {:?} {:?}",
            f[0],
            f[1]
        );
        n += 1;
    }
    assert_eq!(n, 90);
    for f in fields("sep", IDENT) {
        let (input, sep) = (unesc(f[0]), f[1].chars().next().unwrap());
        assert_eq!(
            id_res(Identifier::by_separator(&input, sep)),
            f[2],
            "by_separator {:?} {sep}",
            f[0]
        );
        assert_eq!(
            opt(Identifier::try_by_separator(&input, sep)),
            f[3],
            "try_by_separator {:?} {sep}",
            f[0]
        );
    }
    let base = Identifier::parse("mod:base/x").unwrap();
    for f in fields("with", IDENT) {
        let s = unesc(f[0]);
        assert_eq!(id_res(base.with_path(&s)).trim_start_matches("ok:"), f[1]);
        assert_eq!(id_res(base.with_prefix(&s)).trim_start_matches("ok:"), f[2]);
        assert_eq!(id_res(base.with_suffix(&s)).trim_start_matches("ok:"), f[3]);
    }
    for f in fields("read", IDENT) {
        let got = match Identifier::read(&unesc(f[0])) {
            Ok(i) => i.to_string(),
            Err(e) => format!("err:{e}"),
        };
        assert_eq!(esc(&got), f[1]);
    }
}

#[test]
fn identifier_ordering_matches_vanilla() {
    let mut ids: Vec<Identifier> = fields("id", IDENT)
        .filter_map(|f| Identifier::try_parse(&unesc(f[0])))
        .collect();
    ids.sort();
    let sorted = IDENT
        .lines()
        .find_map(|l| l.strip_prefix("sorted\t"))
        .unwrap();
    let got: Vec<String> = ids.iter().map(|i| esc(&i.to_string())).collect();
    assert_eq!(got.join(" "), sorted);
    for f in fields("cmp", IDENT) {
        let (a, b) = (
            Identifier::parse(f[0]).unwrap(),
            Identifier::parse(f[1]).unwrap(),
        );
        let sign = match a.cmp(&b) {
            Ordering::Less => "-1",
            Ordering::Equal => "0",
            Ordering::Greater => "1",
        };
        assert_eq!(sign, f[2], "{a} vs {b}");
        assert_eq!((a == b).to_string(), f[3]);
    }
}

#[test]
fn identifier_serde_is_a_string() {
    let id = Identifier::parse("mod:a/b").unwrap();
    assert_eq!(serde_json::to_string(&id).unwrap(), "\"mod:a/b\"");
    assert_eq!(
        serde_json::from_str::<Identifier>("\"stone\"")
            .unwrap()
            .to_string(),
        "minecraft:stone"
    );
    let err = serde_json::from_str::<Identifier>("\"Bad:Id\"")
        .unwrap_err()
        .to_string();
    assert!(err.starts_with("Not a valid resource location: Bad:Id Non [a-z0-9_.-] character in namespace of identifier: Bad:Id"), "{err}");
}

#[test]
fn offline_uuids_match_vanilla() {
    let mut n = 0;
    for f in fields("offline", UUIDS) {
        let name = unesc(f[0]);
        let u = create_offline_player_uuid(&name);
        assert_eq!(u.to_string(), f[1], "{:?}", f[0]);
        assert_eq!(most_significant_bits(&u).to_string(), f[2]);
        assert_eq!(least_significant_bits(&u).to_string(), f[3]);
        let ints = uuid_to_int_array(&u);
        assert_eq!(format!("{ints:?}").replace(' ', ""), f[4]);
        assert_eq!(uuid_from_int_array(ints), u);
        let bytes: String = uuid_to_byte_array(&u)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(bytes, f[5]);
        assert_eq!(undashed_to_string(&u), f[6]);
        assert_eq!(java_hash_code(&u).to_string(), f[7]);
        assert_eq!(u.get_version_num().to_string(), f[8]);
        n += 1;
    }
    assert!(n >= 15);
}

#[test]
fn int_array_round_trip_matches_vanilla() {
    for f in fields("ints", UUIDS) {
        let a: Vec<i32> = f[0]
            .trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .map(|x| x.parse().unwrap())
            .collect();
        let u = uuid_from_int_array([a[0], a[1], a[2], a[3]]);
        assert_eq!(u.to_string(), f[1]);
        assert_eq!(
            format!("{:?}", uuid_to_int_array(&u)).replace(' ', ""),
            f[2]
        );
    }
}

fn res(r: Result<Uuid, String>) -> String {
    match r {
        Ok(u) => format!(
            "ok {u} {} {} {}",
            most_significant_bits(&u),
            least_significant_bits(&u),
            java_hash_code(&u)
        ),
        Err(e) => format!("err {}", esc(&e)),
    }
}

#[test]
fn uuid_from_string_matches_vanilla() {
    let mut n = 0;
    for f in fields("from", UUIDS) {
        assert_eq!(res(uuid_from_string(&unesc(f[0]))), f[1], "{:?}", f[0]);
        n += 1;
    }
    assert!(n > 30);
    for f in fields("undashed", UUIDS) {
        let s = unesc(f[0]);
        assert_eq!(res(undashed_from_string(&s)), f[1], "strict {:?}", f[0]);
        assert_eq!(
            res(undashed_from_string_lenient(&s)),
            f[2],
            "lenient {:?}",
            f[0]
        );
    }
}
