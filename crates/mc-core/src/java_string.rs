//! Java `String` byte-level behaviour that differs from Rust's `String`.

/// `new String(bytes, StandardCharsets.UTF_8)` (what Netty's `ByteBuf.toString(UTF_8)` does): malformed input becomes
/// U+FFFD, but Java and Rust disagree on how many replacement characters a malformed sequence yields (Java replaces
/// an encoded surrogate `ed a0 80` by one, Rust by three). The result never holds a lone surrogate.
pub fn decode_utf8(bytes: &[u8]) -> String {
    // Well-formed UTF-8 decodes identically.
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_owned();
    }
    let n = bytes.len();
    let mut out = String::with_capacity(n);
    let cont = |b: u8| b & 0xC0 == 0x80;
    let mut sp = 0;
    while sp < n {
        let b1 = bytes[sp];
        sp += 1;
        if b1 < 0x80 {
            out.push(b1 as char);
        } else if b1 >> 5 == 0b110 && b1 & 0x1e != 0 {
            match bytes.get(sp) {
                Some(&b2) if cont(b2) => {
                    out.push(
                        char::from_u32(u32::from(b1 & 0x1f) << 6 | u32::from(b2 & 0x3f)).unwrap(),
                    );
                    sp += 1;
                }
                Some(_) => out.push('\u{FFFD}'),
                None => {
                    out.push('\u{FFFD}');
                    break;
                }
            }
        } else if b1 >> 4 == 0b1110 {
            if sp + 1 < n {
                let (b2, b3) = (bytes[sp], bytes[sp + 1]);
                if (b1 == 0xE0 && b2 & 0xE0 == 0x80) || !cont(b2) || !cont(b3) {
                    out.push('\u{FFFD}');
                    // JDK `malformed3`: skip 1 byte when the second byte cannot continue, else 2
                    sp += if (b1 == 0xE0 && b2 & 0xE0 == 0x80) || !cont(b2) {
                        0
                    } else {
                        1
                    };
                } else {
                    let c = u32::from(b1 & 0x0f) << 12
                        | u32::from(b2 & 0x3f) << 6
                        | u32::from(b3 & 0x3f);
                    // an encoded surrogate is one malformed sequence of three bytes
                    out.push(char::from_u32(c).unwrap_or('\u{FFFD}'));
                    sp += 2;
                }
            } else if sp < n && ((b1 == 0xE0 && bytes[sp] & 0xE0 == 0x80) || !cont(bytes[sp])) {
                out.push('\u{FFFD}');
            } else {
                out.push('\u{FFFD}');
                break;
            }
        } else if b1 >> 3 == 0b11110 {
            if sp + 2 < n {
                let (b2, b3, b4) = (bytes[sp], bytes[sp + 1], bytes[sp + 2]);
                let uc = u32::from(b1 & 0x07) << 18
                    | u32::from(b2 & 0x3f) << 12
                    | u32::from(b3 & 0x3f) << 6
                    | u32::from(b4 & 0x3f);
                if !cont(b2) || !cont(b3) || !cont(b4) || !(0x10000..=0x10FFFF).contains(&uc) {
                    out.push('\u{FFFD}');
                    // JDK `malformed4`
                    sp += if b1 > 0xF4
                        || (b1 == 0xF0 && !(0x90..=0xBF).contains(&b2))
                        || (b1 == 0xF4 && b2 & 0xF0 != 0x80)
                        || !cont(b2)
                    {
                        0
                    } else if !cont(b3) {
                        1
                    } else {
                        2
                    };
                } else {
                    out.push(char::from_u32(uc).unwrap());
                    sp += 3;
                }
            } else if b1 > 0xF4
                || bytes.get(sp).is_some_and(|&b2| {
                    (b1 == 0xF0 && !(0x90..=0xBF).contains(&b2))
                        || (b1 == 0xF4 && b2 & 0xF0 != 0x80)
                        || !cont(b2)
                })
            {
                out.push('\u{FFFD}');
            } else {
                out.push('\u{FFFD}');
                sp += 1;
                if bytes.get(sp).is_some_and(|&b3| !cont(b3)) {
                    continue;
                }
                break;
            }
        } else {
            out.push('\u{FFFD}');
        }
    }
    out
}
