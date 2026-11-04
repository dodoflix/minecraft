//! Differential test against vanilla: `fixtures/snbt_golden.tsv` is produced by
//! `fixtures/SnbtOracle.java` running the real `TagParser`, printers and `NbtIo`.

use mc_nbt::{nbt_io, snbt, Tag};

fn unesc(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next().unwrap() {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            '\\' => out.push('\\'),
            'x' => {
                let h: String = it.by_ref().take(2).collect();
                out.push(char::from_u32(u32::from_str_radix(&h, 16).unwrap()).unwrap());
            }
            o => panic!("bad escape {o}"),
        }
    }
    out
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn snbt_matches_vanilla() {
    let data = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/snbt_golden.tsv")).unwrap();
    let mut failures = Vec::new();
    let mut n = 0;
    for line in data.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f.len(), 7, "bad line {line}");
        let input = unesc(f[0]);
        n += 1;
        let result = snbt::parse(&input);
        let mut problems = Vec::new();
        match (f[1], &result) {
            ("O", Ok(tag)) => {
                let (compact, pretty) = (unesc(f[2]), unesc(f[3]));
                if snbt::print_compact(tag) != compact {
                    problems.push(format!("compact: got {} want {}", snbt::print_compact(tag), compact));
                }
                if snbt::print_pretty(tag) != pretty {
                    problems.push(format!("pretty: got {:?} want {:?}", snbt::print_pretty(tag), pretty));
                }
                let mut any = Vec::new();
                nbt_io::write_any_tag(tag, &mut any).unwrap();
                if hex(&any) != f[4] {
                    problems.push(format!("any bytes: got {} want {}", hex(&any), f[4]));
                }
                if let Tag::Compound(c) = tag
                    && hex(&nbt_io::to_bytes(c)) != f[5]
                {
                    problems.push(format!("named bytes: got {} want {}", hex(&nbt_io::to_bytes(c)), f[5]));
                }
            }
            ("E", Err(e)) => {
                if e.to_string() != unesc(f[2]) {
                    problems.push(format!("message: got {:?} want {:?}", e.to_string(), unesc(f[2])));
                }
                if e.cursor().to_string() != f[3] {
                    problems.push(format!("cursor: got {} want {}", e.cursor(), f[3]));
                }
            }
            (want, got) => problems.push(format!("status: want {want}, got {got:?}")),
        }
        let compound = snbt::parse_compound(&input);
        match (&compound, f[6].split_at(1)) {
            (Ok(_), ("O", _)) => {}
            (Err(e), ("E", msg)) if e.to_string() == unesc(msg) => {}
            (got, want) => problems.push(format!("compound: got {got:?} want {want:?}")),
        }
        if !problems.is_empty() {
            failures.push(format!("input {input:?}:\n    {}", problems.join("\n    ")));
        }
    }
    if !failures.is_empty() {
        panic!("{} of {n} cases differ from vanilla; first {}:\n{}", failures.len(), failures.len().min(25), failures.iter().take(25).cloned().collect::<Vec<_>>().join("\n"));
    }
}
