//! Java number-parsing semantics used by `SnbtGrammar` (`Integer.parseInt`, `parseUnsignedInt`,
//! Guava `UnsignedBytes.parseUnsignedByte`, ...), including `NumberFormatException` messages.
//! Inputs are ASCII digit strings with an optional leading `-` (the grammar guarantees this).

fn for_input_string(s: &str, radix: u32) -> String {
    if radix == 10 {
        format!("For input string: \"{s}\"")
    } else {
        format!("For input string: \"{s}\" under radix {radix}")
    }
}

fn digit(c: char, radix: u32) -> Option<u32> {
    c.to_digit(radix)
}

/// Shared body of `Integer.parseInt` / `Long.parseLong`; `bits` is 32 or 64.
fn parse_signed(s: &str, radix: u32, bits: u32) -> Option<i64> {
    let (neg, digits) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    if digits.is_empty() {
        return None;
    }
    let (min, max): (i128, i128) = if bits == 32 {
        (i32::MIN as i128, i32::MAX as i128)
    } else {
        (i64::MIN as i128, i64::MAX as i128)
    };
    let mut acc: i128 = 0;
    for c in digits.chars() {
        let d = digit(c, radix)? as i128;
        acc = acc * radix as i128 + d;
        if acc > max + 1 {
            return None;
        }
    }
    let v = if neg { -acc } else { acc };
    (min..=max).contains(&v).then_some(v as i64)
}

pub fn parse_int(s: &str, radix: u32) -> Result<i32, String> {
    parse_signed(s, radix, 32)
        .map(|v| v as i32)
        .ok_or_else(|| for_input_string(s, radix))
}

pub fn parse_long(s: &str, radix: u32) -> Result<i64, String> {
    parse_signed(s, radix, 64).ok_or_else(|| for_input_string(s, radix))
}

pub fn parse_byte(s: &str, radix: u32) -> Result<i8, String> {
    let i = parse_int(s, radix)?;
    i8::try_from(i).map_err(|_| format!("Value out of range. Value:\"{s}\" Radix:{radix}"))
}

pub fn parse_short(s: &str, radix: u32) -> Result<i16, String> {
    let i = parse_int(s, radix)?;
    i16::try_from(i).map_err(|_| format!("Value out of range. Value:\"{s}\" Radix:{radix}"))
}

/// Guava `UnsignedBytes.parseUnsignedByte`.
pub fn parse_unsigned_byte(s: &str, radix: u32) -> Result<i8, String> {
    let i = parse_int(s, radix)?;
    if i >> 8 == 0 {
        Ok(i as i8)
    } else {
        Err(format!("out of range: {i}"))
    }
}

/// `SnbtGrammar.parseUnsignedShort`.
pub fn parse_unsigned_short(s: &str, radix: u32) -> Result<i16, String> {
    let i = parse_int(s, radix)?;
    if i >> 16 == 0 {
        Ok(i as i16)
    } else {
        Err(format!("out of range: {i}"))
    }
}

/// `Integer.parseUnsignedInt` / `Long.parseUnsignedLong` for digit strings that are valid in
/// `radix` (the grammar guarantees it): anything above `max` reports `exceeds range`.
fn parse_unsigned(s: &str, radix: u32, max: u128, what: &str) -> Result<u128, String> {
    if s.is_empty() {
        return Err(for_input_string("", radix));
    }
    if s.starts_with('-') {
        return Err(format!(
            "Illegal leading minus sign on unsigned string {s}."
        ));
    }
    let mut acc: u128 = 0;
    for c in s.chars() {
        let d = digit(c, radix).ok_or_else(|| for_input_string(s, radix))? as u128;
        acc = acc * radix as u128 + d;
        if acc > max {
            // keep validating the remaining digits would not change the outcome for grammar input
            return Err(format!(
                "String value {s} exceeds range of unsigned {what}."
            ));
        }
    }
    Ok(acc)
}

pub fn parse_unsigned_int(s: &str, radix: u32) -> Result<i32, String> {
    parse_unsigned(s, radix, u32::MAX as u128, "int").map(|v| v as u32 as i32)
}

pub fn parse_unsigned_long(s: &str, radix: u32) -> Result<i64, String> {
    parse_unsigned(s, radix, u64::MAX as u128, "long").map(|v| v as u64 as i64)
}

/// `Long.parseLong(CharSequence, begin, end, radix)`; `s` is ASCII.
fn parse_long_range(s: &str, begin: usize, end: usize, radix: u32) -> Result<i64, String> {
    let err = |i: usize| format!("Error at index {} in: \"{}\"", i - begin, &s[begin..end]);
    if begin >= end {
        return Err(String::new());
    }
    let bytes = s.as_bytes();
    let mut i = begin;
    let mut negative = false;
    let mut limit = -i64::MAX;
    let first = bytes[i] as char;
    if first < '0' {
        if first == '-' {
            negative = true;
            limit = i64::MIN;
        } else if first != '+' {
            return Err(err(i));
        }
        i += 1;
        if i >= end {
            return Err(err(i));
        }
    }
    let multmin = limit / radix as i64;
    let mut result: i64 = 0;
    while i < end {
        let d = match digit(bytes[i] as char, radix) {
            Some(d) => d as i64,
            None => return Err(err(i)),
        };
        if result < multmin {
            return Err(err(i));
        }
        result *= radix as i64;
        if result < limit + d {
            return Err(err(i));
        }
        i += 1;
        result -= d;
    }
    Ok(if negative { result } else { -result })
}

/// `java.util.UUID.fromString` (JDK 17+ lenient form). Returns `(most, least)` significant bits.
pub fn uuid_from_string(name: &[u16]) -> Option<(i64, i64)> {
    if name.len() > 36 {
        return None;
    }
    // Non-ASCII digits are not accepted here (Character.digit would accept some).
    if name.iter().any(|&u| u > 0x7f) {
        return None;
    }
    let s: String = name.iter().map(|&u| u as u8 as char).collect();
    let find = |from: usize| s[from..].find('-').map(|p| p + from);
    let dash1 = find(0)?;
    let dash2 = find(dash1 + 1)?;
    let dash3 = find(dash2 + 1)?;
    let dash4 = find(dash3 + 1)?;
    if find(dash4 + 1).is_some() {
        return None;
    }
    let p = |b: usize, e: usize| parse_long_range(&s, b, e, 16).ok();
    let mut most = p(0, dash1)? & 0xffff_ffff;
    most <<= 16;
    most |= p(dash1 + 1, dash2)? & 0xffff;
    most <<= 16;
    most |= p(dash2 + 1, dash3)? & 0xffff;
    let mut least = p(dash3 + 1, dash4)? & 0xffff;
    least <<= 48;
    least |= p(dash4 + 1, s.len())? & 0xffff_ffff_ffff;
    Some((most, least))
}
