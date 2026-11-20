//! Port of `net.minecraft.core.UUIDUtil` plus the `java.util.UUID` / authlib `UndashedUuid` behaviors it relies on.
//! UUIDs are `uuid::Uuid`; `Display` equals Java's `UUID.toString()`.
//! Deviation: hex parsing accepts ASCII digits only (Java's `Character.digit` also accepts other Unicode digits).
use md5::{Digest, Md5};
use uuid::Uuid;

pub const UUID_BYTES: usize = 16;
const UUID_PREFIX_OFFLINE_PLAYER: &str = "OfflinePlayer:";

pub fn uuid_from_msb_lsb(most: i64, least: i64) -> Uuid {
    Uuid::from_u64_pair(most as u64, least as u64)
}

/// `getMostSignificantBits`
pub fn most_significant_bits(uuid: &Uuid) -> i64 {
    uuid.as_u64_pair().0 as i64
}

/// `getLeastSignificantBits`
pub fn least_significant_bits(uuid: &Uuid) -> i64 {
    uuid.as_u64_pair().1 as i64
}

/// `UUID.hashCode()`
pub fn java_hash_code(uuid: &Uuid) -> i32 {
    let hilo = most_significant_bits(uuid) ^ least_significant_bits(uuid);
    (hilo >> 32) as i32 ^ hilo as i32
}

pub fn uuid_from_int_array(int_array: [i32; 4]) -> Uuid {
    let [a, b, c, d] = int_array;
    uuid_from_msb_lsb(
        i64::from(a) << 32 | i64::from(b as u32),
        i64::from(c) << 32 | i64::from(d as u32),
    )
}

pub fn uuid_to_int_array(uuid: &Uuid) -> [i32; 4] {
    least_most_to_int_array(most_significant_bits(uuid), least_significant_bits(uuid))
}

pub fn least_most_to_int_array(most: i64, least: i64) -> [i32; 4] {
    [
        (most >> 32) as i32,
        most as i32,
        (least >> 32) as i32,
        least as i32,
    ]
}

/// Big-endian most then least significant bits.
pub fn uuid_to_byte_array(uuid: &Uuid) -> [u8; 16] {
    *uuid.as_bytes()
}

/// `UUID.nameUUIDFromBytes`: MD5 of the bytes (no namespace prefix) with version 3 / IETF variant bits.
pub fn name_uuid_from_bytes(name: &[u8]) -> Uuid {
    let mut md5: [u8; 16] = Md5::digest(name).into();
    md5[6] = md5[6] & 0x0f | 0x30;
    md5[8] = md5[8] & 0x3f | 0x80;
    Uuid::from_bytes(md5)
}

pub fn create_offline_player_uuid(player_name: &str) -> Uuid {
    name_uuid_from_bytes(format!("{UUID_PREFIX_OFFLINE_PLAYER}{player_name}").as_bytes())
}

/// First code unit of every Unicode decimal digit run (`Nd`) in the BMP: `Character.digit` accepts the ten units from each.
const DIGIT_ZEROS: [u16; 37] = [
    0x0030, 0x0660, 0x06f0, 0x07c0, 0x0966, 0x09e6, 0x0a66, 0x0ae6, 0x0b66, 0x0be6, 0x0c66, 0x0ce6,
    0x0d66, 0x0de6, 0x0e50, 0x0ed0, 0x0f20, 0x1040, 0x1090, 0x17e0, 0x1810, 0x1946, 0x19d0, 0x1a80,
    0x1a90, 0x1b50, 0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0, 0xa9f0, 0xaa50, 0xabf0,
    0xff10,
];

/// `Character.digit(char, 16)`: Unicode decimal digits, ASCII and fullwidth `A-F`/`a-f`.
pub fn java_digit16(c: u16) -> Option<i64> {
    let letter = |base: u16| {
        c.checked_sub(base)
            .filter(|d| *d < 6)
            .map(|d| i64::from(d) + 10)
    };
    DIGIT_ZEROS
        .iter()
        .find_map(|&z| c.checked_sub(z).filter(|d| *d < 10).map(i64::from))
        .or_else(|| letter(0x41))
        .or_else(|| letter(0x61))
        .or_else(|| letter(0xff21))
        .or_else(|| letter(0xff41))
}

/// `Long.parseLong(CharSequence, begin, end, 16)`; error = the `NumberFormatException` message.
fn parse_long_hex(s: &[u16], begin: usize, end: usize) -> Result<i64, String> {
    let seg = || String::from_utf16_lossy(&s[begin..end]);
    let err_at = |i: usize| format!("Error at index {} in: \"{}\"", i - begin, seg());
    if begin >= end {
        return Err("For input string: \"\" under radix 16".to_owned());
    }
    let mut i = begin;
    let mut negative = false;
    let mut limit = -i64::MAX;
    if s[i] < u16::from(b'0') {
        match s[i] {
            c if c == u16::from(b'-') => {
                negative = true;
                limit = i64::MIN;
            }
            c if c == u16::from(b'+') => {}
            _ => return Err(err_at(i)),
        }
        i += 1;
        if i == end {
            return Err(err_at(i));
        }
    }
    let multmin = limit / 16;
    let mut result = 0i64;
    while i < end {
        let Some(digit) = java_digit16(s[i]) else {
            return Err(err_at(i));
        };
        if result < multmin {
            return Err(err_at(i));
        }
        result *= 16;
        if result < limit + digit {
            return Err(err_at(i));
        }
        i += 1;
        result -= digit;
    }
    Ok(if negative { result } else { -result })
}

/// `UUID.fromString`; `Err` carries the Java exception message.
pub fn uuid_from_string(name: &str) -> Result<Uuid, String> {
    let s: Vec<u16> = name.encode_utf16().collect();
    let len = s.len();
    if len > 36 {
        return Err("UUID string too large".to_owned());
    }
    let dash = b'-' as u16;
    let index_of = |from: usize| {
        s.get(from..)
            .and_then(|t| t.iter().position(|&c| c == dash))
            .map(|p| p + from)
    };
    let dash1 = index_of(0);
    let dash2 = dash1.and_then(|d| index_of(d + 1));
    let dash3 = dash2.and_then(|d| index_of(d + 1));
    let dash4 = dash3.and_then(|d| index_of(d + 1));
    let dash5 = dash4.and_then(|d| index_of(d + 1));
    let (Some(d1), Some(d2), Some(d3), Some(d4), None) = (dash1, dash2, dash3, dash4, dash5) else {
        return Err(format!("Invalid UUID string: {name}"));
    };
    let mut most = parse_long_hex(&s, 0, d1)? & 0xffff_ffff;
    most <<= 16;
    most |= parse_long_hex(&s, d1 + 1, d2)? & 0xffff;
    most <<= 16;
    most |= parse_long_hex(&s, d2 + 1, d3)? & 0xffff;
    let mut least = parse_long_hex(&s, d3 + 1, d4)? & 0xffff;
    least <<= 48;
    least |= parse_long_hex(&s, d4 + 1, len)? & 0xffff_ffff_ffff;
    Ok(uuid_from_msb_lsb(most, least))
}

/// authlib `UndashedUuid.toString`
pub fn undashed_to_string(uuid: &Uuid) -> String {
    uuid.as_simple().to_string()
}

/// authlib `UndashedUuid.fromString`
pub fn undashed_from_string(s: &str) -> Result<Uuid, String> {
    if s.contains('-') {
        return Err(format!("Invalid undashed UUID string: {s}"));
    }
    undashed_from_string_lenient(s)
}

/// authlib `UndashedUuid.fromStringLenient`: first run of 32 `\w` chars gets dashes inserted, then `UUID.fromString`.
pub fn undashed_from_string_lenient(s: &str) -> Result<Uuid, String> {
    let chars: Vec<char> = s.chars().collect();
    let is_word = |c: &char| c.is_ascii_alphanumeric() || *c == '_';
    let mut run = 0;
    let mut start = None;
    for (i, c) in chars.iter().enumerate() {
        run = if is_word(c) { run + 1 } else { 0 };
        if run == 32 {
            start = Some(i + 1 - 32);
            break;
        }
    }
    let Some(start) = start else {
        return uuid_from_string(s);
    };
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in chars.iter().enumerate() {
        if matches!(i.wrapping_sub(start), 8 | 12 | 16 | 20) {
            out.push('-');
        }
        out.push(*c);
    }
    uuid_from_string(&out)
}
