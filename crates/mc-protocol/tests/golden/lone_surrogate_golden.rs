//! Network strings with lone UTF-16 surrogates and malformed UTF-8 against vanilla (`Utf8String`, Netty's UTF-8 codec):
//! `fixtures/lone_surrogate.tsv`, recorded by `fixtures/lone_surrogate/LoneSurrogateGolden.java`.

use mc_protocol::codec::{read_utf, write_utf};

const GOLDEN: &str = include_str!("../fixtures/lone_surrogate.tsv");

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(h: &str) -> Vec<u8> {
    (0..h.len() / 2)
        .map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

/// Inverse of the recorder's `esc`, as UTF-16 units.
fn unesc_units(s: &str) -> Vec<u16> {
    let mut units: Vec<u16> = Vec::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            units.extend(c.encode_utf16(&mut [0; 2]).iter());
            continue;
        }
        units.push(match chars.next().unwrap() {
            '\\' => u16::from(b'\\'),
            't' => u16::from(b'\t'),
            'n' => u16::from(b'\n'),
            'r' => u16::from(b'\r'),
            'u' => u16::from_str_radix(&chars.by_ref().take(4).collect::<String>(), 16).unwrap(),
            other => panic!("bad escape \\{other}"),
        });
    }
    units
}

fn rows(op: &str) -> impl Iterator<Item = Vec<&'static str>> + '_ {
    GOLDEN
        .lines()
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .filter(move |c| c[0] == op)
}

fn read(payload: &[u8]) -> String {
    let mut framed = vec![payload.len() as u8];
    framed.extend_from_slice(payload);
    read_utf(&mut &framed[..], 32767).unwrap()
}

#[test]
fn network_strings_decode_like_java() {
    let mut n = 0;
    for op in ["net_read", "net_read_all"] {
        for c in rows(op) {
            let want = String::from_utf16(&unesc_units(c[2]))
                .expect("the decoder yields no lone surrogate");
            assert_eq!(read(&unhex(c[1])), want, "{op} {}", c[1]);
            n += 1;
        }
    }
    assert!(n > 16_000, "{n} rows");
}

#[test]
fn network_strings_without_lone_surrogates_encode_like_vanilla() {
    let mut n = 0;
    for c in rows("net_write") {
        let Ok(s) = String::from_utf16(&unesc_units(c[1])) else {
            continue;
        };
        let mut out = Vec::new();
        write_utf(&mut out, &s, 32767).unwrap();
        assert_eq!(hex(&out), c[2], "net_write {}", c[1]);
        assert_eq!(read(&out[1..]), s);
        n += 1;
    }
    for c in rows("net_write_max2") {
        let Ok(s) = String::from_utf16(&unesc_units(c[1])) else {
            continue;
        };
        let mut out = Vec::new();
        let got = match write_utf(&mut out, &s, 2) {
            Ok(()) => hex(&out),
            Err(e) => format!("ERR:EncoderException:{}", e.0),
        };
        assert_eq!(got, c[2], "net_write_max2 {}", c[1]);
        n += 1;
    }
    assert!(n >= 18, "{n} rows");
}
