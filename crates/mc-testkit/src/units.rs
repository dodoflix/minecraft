//! Helpers for fixtures that record Java strings: `esc` writes UTF-16 code units outside printable ASCII as `\uXXXX`
//! (so lone surrogates are visible), binary results are lowercase hex, rows are tab-separated.

/// Lowercase hex of `bytes`.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The bytes of a hex string.
pub fn unhex(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

/// The UTF-16 units of a recorded `esc`aped string.
pub fn unesc_units(s: &str) -> Vec<u16> {
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

/// The tab-separated rows of `golden` whose first column is `op`.
pub fn rows<'a>(golden: &'a str, op: &'a str) -> impl Iterator<Item = Vec<&'a str>> + 'a {
    golden
        .lines()
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .filter(move |c| c[0] == op)
}
