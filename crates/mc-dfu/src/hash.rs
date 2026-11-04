//! `net.minecraft.util.HashOps` (CRC32C): the hash of a value tree built with the hash ops.

use crate::dynv::{Dyn, JNum};

const POLY: u32 = 0x82F6_3B78;

fn table() -> &'static [u32; 256] {
    static T: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { (c >> 1) ^ POLY } else { c >> 1 };
            }
            *e = c;
        }
        t
    })
}

/// CRC32C of `data` (Guava `Hashing.crc32c()`; `HashCode.asInt()`).
pub fn crc32c(data: &[u8]) -> u32 {
    let t = table();
    let mut c = !0u32;
    for &b in data {
        c = t[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}

/// Accumulates the bytes a Guava `Hasher` would see, then hashes them.
#[derive(Default)]
struct Hasher(Vec<u8>);

impl Hasher {
    fn byte(&mut self, b: u8) -> &mut Self {
        self.0.push(b);
        self
    }
    fn int(&mut self, v: i32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn hash(&self) -> u32 {
        crc32c(&self.0)
    }
}

fn leaf(tag: u8, payload: &[u8]) -> u32 {
    let mut h = Hasher::default();
    h.byte(tag);
    h.0.extend_from_slice(payload);
    h.hash()
}

pub(crate) fn hash_number(n: &JNum) -> u32 {
    match n {
        JNum::Byte(v) => leaf(6, &v.to_le_bytes()),
        JNum::Short(v) => leaf(7, &v.to_le_bytes()),
        JNum::Int(v) => leaf(8, &v.to_le_bytes()),
        JNum::Long(v) => leaf(9, &v.to_le_bytes()),
        JNum::Float(v) => leaf(10, &v.to_bits().to_le_bytes()),
        JNum::Double(v) => leaf(11, &v.to_bits().to_le_bytes()),
        // `createNumeric` default branch
        JNum::Lazy(_) => leaf(11, &n.double_value().to_bits().to_le_bytes()),
    }
}

pub(crate) fn hash_string(s: &str) -> u32 {
    let mut h = Hasher::default();
    h.byte(12);
    let units: Vec<u16> = s.encode_utf16().collect();
    h.int(units.len() as i32);
    for u in units {
        h.0.extend_from_slice(&u.to_le_bytes());
    }
    h.hash()
}

fn hash_map(entries: &[(u32, u32)]) -> u32 {
    let mut e = entries.to_vec();
    e.sort_by_key(|&(k, v)| (k, v)); // unsigned (padToLong) key, then value
    let mut h = Hasher::default();
    h.byte(2);
    for (k, v) in e {
        h.0.extend_from_slice(&k.to_le_bytes());
        h.0.extend_from_slice(&v.to_le_bytes());
    }
    h.byte(3);
    h.hash()
}

/// Hash of a tree produced by hash-kind ops (`HashOps.CRC32C_INSTANCE`), as `HashCode.asInt()`.
pub fn hash_dyn(d: &Dyn) -> u32 {
    match d {
        Dyn::Empty => leaf(1, &[]),
        Dyn::Bool(b) => crc32c(&[13, *b as u8]),
        Dyn::Byte(v) => hash_number(&JNum::Byte(*v)),
        Dyn::Short(v) => hash_number(&JNum::Short(*v)),
        Dyn::Int(v) => hash_number(&JNum::Int(*v)),
        Dyn::Long(v) => hash_number(&JNum::Long(*v)),
        Dyn::Float(v) => hash_number(&JNum::Float(*v)),
        Dyn::Double(v) => hash_number(&JNum::Double(*v)),
        Dyn::JsonNum(t) => hash_number(&JNum::Lazy(t.clone())),
        Dyn::Str(s) => hash_string(s),
        Dyn::List(l) => {
            let mut h = Hasher::default();
            h.byte(4);
            for e in l {
                h.0.extend_from_slice(&hash_dyn(e).to_le_bytes());
            }
            h.byte(5);
            h.hash()
        }
        Dyn::Map(m) => {
            let e: Vec<(u32, u32)> = m.iter().map(|(k, v)| (hash_dyn(k), hash_dyn(v))).collect();
            hash_map(&e)
        }
        Dyn::Bytes(v) => {
            let mut h = Hasher::default();
            h.byte(14);
            h.0.extend(v.iter().map(|b| *b as u8));
            h.byte(15);
            h.hash()
        }
        Dyn::Ints(v) => {
            let mut h = Hasher::default();
            h.byte(16);
            for i in v {
                h.int(*i);
            }
            h.byte(17);
            h.hash()
        }
        Dyn::Longs(v) => {
            let mut h = Hasher::default();
            h.byte(18);
            for i in v {
                h.0.extend_from_slice(&i.to_le_bytes());
            }
            h.byte(19);
            h.hash()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32c_check_value() {
        assert_eq!(crc32c(b"123456789"), 0xE306_9283);
    }
}
